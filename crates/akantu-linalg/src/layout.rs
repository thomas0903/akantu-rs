// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/dof_manager.hh
// (global_equation_number, global_to_local_mapping, dofs_flag,
// isLocalOrMasterDOF, globalToLocalEquationNumber,
// localToGlobalEquationNumber), dof_manager.cc:768-870 (computeFirstDOFIDs,
// updateDOFsData).

//! Equation layout: the explicit map between the **local equations** of this
//! rank and the **global equations** of the distributed system
//! (`docs/ARCHITECTURE.md`).
//!
//! Three index spaces are kept apart and never mixed in one API:
//!
//! | Space | Type | Meaning |
//! |---|---|---|
//! | local entity | `Idx` (node, element) | mesh numbering of this rank |
//! | local equation | `usize` in memory, `Idx` when stored | row of this rank's vectors (`SolverVector`) |
//! | global equation | [`GlobalIdx`] | row/column of the distributed matrix (COO coordinates) |
//!
//! Every local equation has an [`EquationOwnership`]: owned by this rank
//! (upstream "local or master" DOF), shared (a slave copy of an equation
//! owned elsewhere — it has the owner's global number) or a pure ghost (no
//! global equation at all: upstream global number `-1`). Sparse matrices
//! store **global** coordinates; the layout translates them back to local
//! rows for products, Dirichlet handling and solver input.

use akantu_core::{Error, GlobalIdx, INVALID_GLOBAL_IDX, Result};

use crate::int_hash::IntHashMap;

/// Ownership of one local equation (upstream `dofs_flag` + global number).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EquationOwnership {
    /// This rank owns the equation (upstream `isLocalOrMasterDOF`).
    Owned,
    /// Copy of an equation owned by another rank (upstream slave DOF): it
    /// carries the owner's global equation number.
    Shared,
    /// Ghost equation without a global number (pure ghost node); it never
    /// enters a matrix or a global vector.
    PureGhost,
}

/// Explicit local ↔ global equation map of one rank.
///
/// Invariants (checked by [`EquationLayout::validate`]):
/// * `local_to_global.len() == ownership.len()`;
/// * a pure-ghost equation has `INVALID_GLOBAL_IDX`, every other equation a
///   valid global number `< system_size`;
/// * owned global numbers are unique on this rank; `global_to_local` maps
///   every valid global number present on this rank to its **first** local
///   equation (periodic slaves alias their master's local equation, so no
///   two local equations share a global number).
#[derive(Clone, Debug, PartialEq)]
pub struct EquationLayout {
    local_to_global: Vec<GlobalIdx>,
    ownership: Vec<EquationOwnership>,
    global_to_local: GlobalToLocal,
    system_size: GlobalIdx,
    /// Persistent global offset for the next registration/addition (upstream
    /// `DOFManager::first_global_dof_id`): numbering is per update, not a
    /// single rank-blocked range.
    next_global: GlobalIdx,
    /// Every local equation is owned, `local_to_global[l] == l` and the
    /// system has `nb_local` equations (a serial layout without periodic
    /// aliases): global and local numbers coincide. Derived from the other
    /// fields, maintained by every mutation.
    identity: bool,
}

impl Default for EquationLayout {
    fn default() -> Self {
        Self {
            local_to_global: Vec::new(),
            ownership: Vec::new(),
            global_to_local: GlobalToLocal::default(),
            system_size: 0,
            next_global: 0,
            identity: true,
        }
    }
}

/// Sentinel of an absent global number in [`GlobalToLocal::Dense`].
const ABSENT: u32 = u32::MAX;

/// Global → first local equation map. Dense (a `Vec` indexed by the global
/// number) while the largest global number present stays below
/// `4 · nb_local + 4096`, hashed otherwise (a rank of a large distributed
/// system holds a few scattered global numbers). Both answer the same
/// queries; only the cost differs.
#[derive(Clone, Debug)]
enum GlobalToLocal {
    Dense(Vec<u32>),
    Hashed(IntHashMap<GlobalIdx, usize>),
}

impl Default for GlobalToLocal {
    fn default() -> Self {
        Self::Dense(Vec::new())
    }
}

impl PartialEq for GlobalToLocal {
    fn eq(&self, other: &Self) -> bool {
        self.pairs() == other.pairs()
    }
}

impl GlobalToLocal {
    /// Largest dense length allowed for `nb_local` local equations.
    fn dense_limit(nb_local: usize) -> usize {
        nb_local.saturating_mul(4).saturating_add(4096)
    }

    #[inline]
    fn get(&self, g: GlobalIdx) -> Option<usize> {
        match self {
            Self::Dense(v) => {
                let l = *v.get(usize::try_from(g).ok()?)?;
                (l != ABSENT).then_some(l as usize)
            }
            Self::Hashed(m) => m.get(&g).copied(),
        }
    }

    /// Record `g → l` unless `g` is present (first local equation wins);
    /// `nb_local` is the number of local equations after the change.
    fn insert_first(&mut self, g: GlobalIdx, l: usize, nb_local: usize) {
        if let Self::Dense(v) = self {
            let fits = usize::try_from(g)
                .ok()
                .filter(|&gi| gi < Self::dense_limit(nb_local))
                .zip(u32::try_from(l).ok().filter(|&l32| l32 != ABSENT));
            if let Some((gi, l32)) = fits {
                if gi >= v.len() {
                    v.resize(gi + 1, ABSENT);
                }
                if v[gi] == ABSENT {
                    v[gi] = l32;
                }
                return;
            }
            let m: IntHashMap<GlobalIdx, usize> = self.pairs().into_iter().collect();
            *self = Self::Hashed(m);
        }
        if let Self::Hashed(m) = self {
            m.entry(g).or_insert(l);
        }
    }

    /// Every `(global, local)` pair, sorted (comparisons and conversions).
    fn pairs(&self) -> Vec<(GlobalIdx, usize)> {
        let mut p: Vec<(GlobalIdx, usize)> = match self {
            Self::Dense(v) => v
                .iter()
                .enumerate()
                .filter(|(_, l)| **l != ABSENT)
                .filter_map(|(g, &l)| Some((GlobalIdx::try_from(g).ok()?, l as usize)))
                .collect(),
            Self::Hashed(m) => m.iter().map(|(&g, &l)| (g, l)).collect(),
        };
        p.sort_unstable();
        p
    }
}

fn global_from_usize(n: usize) -> Result<GlobalIdx> {
    GlobalIdx::try_from(n).map_err(|_| {
        Error::InvalidArgument(format!("count {n} does not fit the global index type"))
    })
}

fn checked_add(a: GlobalIdx, b: GlobalIdx) -> Result<GlobalIdx> {
    a.checked_add(b)
        .ok_or_else(|| Error::InvalidArgument("global equation count overflow".into()))
}

impl EquationLayout {
    /// Empty layout.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Serial identity layout of `n` owned equations (`global == local`).
    pub fn serial(n: usize) -> Result<Self> {
        let mut l = Self::new();
        l.append_block(&vec![EquationOwnership::Owned; n], 0, n, |_| None)?;
        Ok(l)
    }

    /// Number of local equations (upstream `local_system_size`).
    #[must_use]
    pub fn nb_local(&self) -> usize {
        self.local_to_global.len()
    }

    /// Number of owned equations (upstream `pure_local_system_size`).
    #[must_use]
    pub fn nb_owned(&self) -> usize {
        self.ownership
            .iter()
            .filter(|o| **o == EquationOwnership::Owned)
            .count()
    }

    /// Global system size (upstream `system_size`).
    #[must_use]
    pub fn system_size(&self) -> GlobalIdx {
        self.system_size
    }

    /// Next global number to hand out (persistent offset).
    #[must_use]
    pub fn next_global(&self) -> GlobalIdx {
        self.next_global
    }

    /// Global number of local equation `l` (`None` for pure ghosts — upstream
    /// `localToGlobalEquationNumber` returning `-1`).
    #[must_use]
    pub fn global(&self, l: usize) -> Option<GlobalIdx> {
        match self.local_to_global.get(l) {
            Some(&g) if g != INVALID_GLOBAL_IDX => Some(g),
            _ => None,
        }
    }

    /// Local equation of global equation `g` (upstream
    /// `globalToLocalEquationNumber`); error if `g` is not present on this
    /// rank.
    pub fn local(&self, g: GlobalIdx) -> Result<usize> {
        self.global_to_local.get(g).ok_or_else(|| {
            Error::InvalidArgument(format!("global equation {g} is not present on this rank"))
        })
    }

    /// Local equation of global equation `g`, `None` if `g` is not present
    /// on this rank ([`EquationLayout::local`] without the error value).
    #[inline]
    #[must_use]
    pub fn try_local(&self, g: GlobalIdx) -> Option<usize> {
        self.global_to_local.get(g)
    }

    /// `true` if global and local equation numbers coincide: every local
    /// equation is owned, local equation `l` has global number `l`, and the
    /// system has [`EquationLayout::nb_local`] equations (a serial layout
    /// without periodic aliases). Then [`EquationLayout::local`]`(g)` is
    /// `g` for every `0 <= g < nb_local` and an error otherwise.
    #[inline]
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.identity
    }

    fn refresh_identity(&mut self, first_new_local: usize) {
        let n = self.local_to_global.len();
        self.identity = self.identity
            && GlobalIdx::try_from(n).is_ok_and(|n| n == self.system_size)
            && self.local_to_global[first_new_local..]
                .iter()
                .zip(&self.ownership[first_new_local..])
                .enumerate()
                .all(|(i, (&g, &o))| {
                    o == EquationOwnership::Owned
                        && GlobalIdx::try_from(first_new_local + i).is_ok_and(|l| l == g)
                });
    }

    /// Ownership of local equation `l`.
    #[must_use]
    pub fn ownership(&self, l: usize) -> Option<EquationOwnership> {
        self.ownership.get(l).copied()
    }

    /// `true` if `l` is owned here (upstream `isLocalOrMasterDOF`).
    #[must_use]
    pub fn is_owned(&self, l: usize) -> bool {
        self.ownership(l) == Some(EquationOwnership::Owned)
    }

    /// Local → global map (pure ghosts hold `INVALID_GLOBAL_IDX`).
    #[must_use]
    pub fn local_to_global(&self) -> &[GlobalIdx] {
        &self.local_to_global
    }

    /// Append a block of `ownership.len()` local equations created by one
    /// registration or node addition (upstream `updateDOFsData`).
    ///
    /// * `first_owned_global` is this rank's first global number for the
    ///   block, computed collectively by the caller as
    ///   `next_global + exclusive_scan(nb_owned)`;
    /// * `total_new_owned` is the all-reduced number of new owned equations
    ///   (all ranks), which advances the persistent offset and the system
    ///   size identically on every rank;
    /// * `shared_global(i)` gives the global number of the `i`-th equation of
    ///   the block when it is [`EquationOwnership::Shared`] and the number is
    ///   already known (otherwise it is filled later by
    ///   [`EquationLayout::set_shared_global`]).
    ///
    /// Owned equations receive consecutive global numbers in block order.
    /// Returns the local index of the first new equation.
    pub fn append_block(
        &mut self,
        ownership: &[EquationOwnership],
        first_owned_global: GlobalIdx,
        total_new_owned: usize,
        mut shared_global: impl FnMut(usize) -> Option<GlobalIdx>,
    ) -> Result<usize> {
        let first_local = self.local_to_global.len();
        let total = global_from_usize(total_new_owned)?;
        let mut next = first_owned_global;
        let mut staged = Vec::with_capacity(ownership.len());
        for (i, o) in ownership.iter().enumerate() {
            let g = match o {
                EquationOwnership::Owned => {
                    let g = next;
                    next = checked_add(next, 1)?;
                    g
                }
                EquationOwnership::Shared => shared_global(i).unwrap_or(INVALID_GLOBAL_IDX),
                EquationOwnership::PureGhost => INVALID_GLOBAL_IDX,
            };
            staged.push(g);
        }
        let new_size = checked_add(self.system_size, total)?;
        if next > checked_add(self.next_global, total)? {
            return Err(Error::InvalidArgument(
                "owned equations exceed the all-reduced count of the block".into(),
            ));
        }
        let next_global = checked_add(self.next_global, total)?;
        // Commit (no partial state on error above).
        let nb_local = first_local + ownership.len();
        for (i, (g, o)) in staged.into_iter().zip(ownership).enumerate() {
            if g != INVALID_GLOBAL_IDX {
                self.global_to_local
                    .insert_first(g, first_local + i, nb_local);
            }
            self.local_to_global.push(g);
            self.ownership.push(*o);
        }
        self.system_size = new_size;
        self.next_global = next_global;
        self.refresh_identity(first_local);
        Ok(first_local)
    }

    /// Fill the global number of a shared equation (received from its owner,
    /// upstream `GlobalDOFInfoDataAccessor::unpackData`). A different,
    /// already-set number is an error (upstream debug assertion).
    pub fn set_shared_global(&mut self, l: usize, g: GlobalIdx) -> Result<()> {
        let cur = *self
            .local_to_global
            .get(l)
            .ok_or_else(|| Error::OutOfRange {
                what: "local equation",
                index: l,
                len: self.nb_local(),
            })?;
        if self.ownership[l] != EquationOwnership::Shared {
            return Err(Error::InvalidArgument(format!(
                "local equation {l} is not a shared equation"
            )));
        }
        if cur != INVALID_GLOBAL_IDX && cur != g {
            return Err(Error::InvalidArgument(format!(
                "shared equation {l} already has global number {cur}, received {g}"
            )));
        }
        self.local_to_global[l] = g;
        self.global_to_local.insert_first(g, l, self.nb_local());
        // a shared equation exists: not an identity layout
        self.identity = false;
        Ok(())
    }

    /// The global number of a periodic master copy this rank holds without
    /// owning it, received through the periodic node synchronizer (upstream
    /// `updateDOFsData`: `_giu_global_conn` on the periodic synchronizer
    /// before `_ask_nodes`, `dof_manager.cc:860-870`). Such a copy may be a
    /// pure-ghost node (appended by the distributed `makePeriodic`): its
    /// equation then becomes a [`EquationOwnership::Shared`] copy carrying
    /// the owner's number, as upstream gives it the owner's global number,
    /// because the local periodic slaves alias it. A
    /// shared equation is checked as by [`EquationLayout::set_shared_global`].
    ///
    /// # Errors
    /// [`Error::OutOfRange`] for an unknown local equation;
    /// [`Error::InvalidArgument`] for an owned equation, a number at or above
    /// the system size, or a different number already set.
    pub fn set_periodic_master_global(&mut self, l: usize, g: GlobalIdx) -> Result<()> {
        let o = *self.ownership.get(l).ok_or_else(|| Error::OutOfRange {
            what: "local equation",
            index: l,
            len: self.nb_local(),
        })?;
        match o {
            EquationOwnership::Owned => Err(Error::InvalidArgument(format!(
                "local equation {l} is owned: no periodic master copy"
            ))),
            EquationOwnership::Shared => self.set_shared_global(l, g),
            EquationOwnership::PureGhost => {
                if g < 0 || g >= self.system_size {
                    return Err(Error::InvalidArgument(format!(
                        "periodic master copy {l}: global number {g} outside the system \
                         (size {})",
                        self.system_size
                    )));
                }
                self.ownership[l] = EquationOwnership::Shared;
                self.local_to_global[l] = g;
                self.global_to_local.insert_first(g, l, self.nb_local());
                self.identity = false;
                Ok(())
            }
        }
    }

    /// Check the invariants listed on the type.
    pub fn validate(&self) -> Result<()> {
        if self.local_to_global.len() != self.ownership.len() {
            return Err(Error::InvalidArgument(
                "layout arrays differ in length".into(),
            ));
        }
        for (l, (&g, &o)) in self.local_to_global.iter().zip(&self.ownership).enumerate() {
            match o {
                EquationOwnership::PureGhost if g != INVALID_GLOBAL_IDX => {
                    return Err(Error::InvalidArgument(format!(
                        "pure ghost equation {l} has global number {g}"
                    )));
                }
                EquationOwnership::Owned | EquationOwnership::Shared
                    if g == INVALID_GLOBAL_IDX || g >= self.system_size =>
                {
                    return Err(Error::InvalidArgument(format!(
                        "equation {l} has invalid global number {g}"
                    )));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Reduction of rank-partial vectors onto owners followed by owner → copy
/// synchronisation (upstream `DOFSynchronizer::reduceSynchronizeArray
/// <AddOperation>`). Implemented by the DOF manager (it owns the DOF
/// synchronizer); serial runs use [`NoReduction`].
pub trait PartialSumReduction {
    /// `true` if the vector is distributed (a reduction is required).
    fn is_distributed(&self) -> bool;
    /// Sum the contributions of shared copies into the owner's entry and
    /// copy the owner's total back to every shared copy; pure ghosts are
    /// left untouched. Collective.
    fn reduce_and_synchronize(&self, local: &mut [f64]) -> Result<()>;
}

/// Serial: nothing to reduce.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoReduction;

impl PartialSumReduction for NoReduction {
    fn is_distributed(&self) -> bool {
        false
    }
    fn reduce_and_synchronize(&self, _local: &mut [f64]) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EquationOwnership::{Owned, PureGhost, Shared};

    /// "First local equation wins" for a global number present twice on
    /// a rank, in the dense map, across its conversion to the hashed map
    /// and in the hashed map; and through the public API (a shared copy
    /// carrying the global number of an owned equation of this rank).
    #[test]
    fn first_local_equation_wins_dense_and_hashed() {
        let mut m = GlobalToLocal::default();
        m.insert_first(5, 0, 2);
        m.insert_first(5, 1, 2);
        assert!(matches!(m, GlobalToLocal::Dense(_)));
        assert_eq!(m.get(5), Some(0));
        // beyond 4 · nb_local + 4096: converts to the hashed map
        m.insert_first(1_000_000, 2, 3);
        assert!(matches!(m, GlobalToLocal::Hashed(_)));
        assert_eq!((m.get(5), m.get(1_000_000)), (Some(0), Some(2)));
        m.insert_first(1_000_000, 3, 4);
        m.insert_first(5, 4, 5);
        assert_eq!((m.get(5), m.get(1_000_000)), (Some(0), Some(2)));
        assert_eq!(m.get(6), None);

        let mut l = EquationLayout::new();
        l.append_block(&[Owned, Owned, Shared], 0, 2, |i| (i == 2).then_some(0))
            .unwrap();
        assert_eq!(l.local(0).unwrap(), 0, "the owned equation, not the copy");
        assert!(!l.is_identity());
    }

    #[test]
    fn serial_identity() {
        let l = EquationLayout::serial(3).unwrap();
        assert_eq!((l.nb_local(), l.nb_owned(), l.system_size()), (3, 3, 3));
        assert_eq!(l.global(2), Some(2));
        assert_eq!(l.local(1).unwrap(), 1);
        assert!(l.local(3).is_err());
        l.validate().unwrap();
    }

    #[test]
    fn blocks_keep_a_persistent_offset_and_pure_ghosts_have_no_global() {
        // Rank 1 of 2: first registration, 2 owned equations of 5 in total,
        // rank 0 owns 3 (exclusive scan = 3).
        let mut l = EquationLayout::new();
        l.append_block(&[Owned, Shared, Owned, PureGhost], 3, 5, |i| {
            (i == 1).then_some(0)
        })
        .unwrap();
        assert_eq!(l.local_to_global()[..3], [3, 0, 4]);
        assert_eq!(l.global(3), None);
        assert_eq!((l.system_size(), l.next_global()), (5, 5));
        // A later addition (e.g. a second DOF family or inserted nodes)
        // continues from the persistent offset, not from a rank block.
        l.append_block(&[Owned], 5 + 1, 2, |_| None).unwrap();
        assert_eq!(l.global(4), Some(6));
        assert_eq!((l.system_size(), l.next_global()), (7, 7));
        l.validate().unwrap();
        assert!(
            l.set_shared_global(1, 1).is_err(),
            "conflicting owner number"
        );
        assert!(l.set_shared_global(0, 1).is_err(), "not a shared equation");
    }

    /// A pure-ghost periodic master copy receives the
    /// owner's global number and becomes a shared equation; an owned
    /// equation is refused, a shared one checked as `set_shared_global`.
    #[test]
    fn periodic_master_copy_receives_the_owners_number() {
        let mut l = EquationLayout::new();
        l.append_block(
            &[
                EquationOwnership::Owned,
                EquationOwnership::Shared,
                EquationOwnership::PureGhost,
            ],
            0,
            3,
            |_| None,
        )
        .unwrap();
        assert!(l.set_periodic_master_global(0, 1).is_err(), "owned");
        assert!(
            l.set_periodic_master_global(2, 3).is_err(),
            "outside the system"
        );
        l.set_periodic_master_global(2, 2).unwrap();
        assert_eq!(l.global(2), Some(2));
        assert!(!l.is_owned(2));
        l.set_periodic_master_global(1, 1).unwrap();
        assert!(
            l.set_periodic_master_global(1, 2).is_err(),
            "different number"
        );
        l.validate().unwrap();
    }
}
