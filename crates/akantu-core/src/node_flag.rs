// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_common.hh (enum class
// NodeFlag and its operators), src/common/aka_common.cc (operator<<),
// src/mesh/mesh_inline_impl.hh (isPureGhostNode, isLocalNode, …).

//! Per-node distribution / periodicity flags.

use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

/// Node flag bit-field (upstream `enum class NodeFlag : std::uint8_t`).
///
/// The low nibble encodes the MPI sharing status, the high nibble the
/// periodicity status. Values are identical to upstream so that flags can be
/// compared with golden fixtures and exchanged with the C++ oracle.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct NodeFlag(pub u8);

impl NodeFlag {
    /// Purely local node.
    pub const NORMAL: Self = Self(0x00);
    /// Shared between ranks (generic bit).
    pub const DISTRIBUTED: Self = Self(0x01);
    /// Shared, owned by this rank.
    pub const MASTER: Self = Self(0x03);
    /// Shared, owned by another rank.
    pub const SLAVE: Self = Self(0x05);
    /// Only referenced by ghost elements.
    pub const PURE_GHOST: Self = Self(0x09);
    /// Mask of the sharing bits.
    pub const SHARED_MASK: Self = Self(0x0F);
    /// Periodic node (generic bit).
    pub const PERIODIC: Self = Self(0x10);
    /// Periodic master.
    pub const PERIODIC_MASTER: Self = Self(0x30);
    /// Periodic slave.
    pub const PERIODIC_SLAVE: Self = Self(0x50);
    /// Debug helper combination (upstream `_pbc_master_dist_master`).
    pub const PBC_MASTER_DIST_MASTER: Self = Self(0x33);
    /// Debug helper combination.
    pub const PBC_MASTER_DIST_SLAVE: Self = Self(0x35);
    /// Debug helper combination.
    pub const PBC_MASTER_DIST_GHOST: Self = Self(0x39);
    /// Debug helper combination.
    pub const PBC_SLAVE_DIST_MASTER: Self = Self(0x53);
    /// Debug helper combination.
    pub const PBC_SLAVE_DIST_SLAVE: Self = Self(0x55);
    /// Debug helper combination.
    pub const PBC_SLAVE_DIST_GHOST: Self = Self(0x59);
    /// Mask of the periodic bits.
    pub const PERIODIC_MASK: Self = Self(0xF0);
    /// Upstream `_local_master_mask` (`0xCC`).
    pub const LOCAL_MASTER_MASK: Self = Self(0xCC);

    /// Raw value.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// `(flag & _shared_mask) == _pure_ghost` (upstream `isPureGhostNode`).
    #[inline]
    #[must_use]
    pub const fn is_pure_ghost(self) -> bool {
        self.0 & Self::SHARED_MASK.0 == Self::PURE_GHOST.0
    }

    /// `(flag & _local_master_mask) == _normal` (upstream `isLocalOrMasterNode`).
    #[inline]
    #[must_use]
    pub const fn is_local_or_master(self) -> bool {
        self.0 & Self::LOCAL_MASTER_MASK.0 == Self::NORMAL.0
    }

    /// `(flag & _shared_mask) == _normal` (upstream `isLocalNode`).
    #[inline]
    #[must_use]
    pub const fn is_local(self) -> bool {
        self.0 & Self::SHARED_MASK.0 == Self::NORMAL.0
    }

    /// `(flag & _shared_mask) == _master` (upstream `isMasterNode`).
    #[inline]
    #[must_use]
    pub const fn is_master(self) -> bool {
        self.0 & Self::SHARED_MASK.0 == Self::MASTER.0
    }

    /// `(flag & _shared_mask) == _slave` (upstream `isSlaveNode`).
    #[inline]
    #[must_use]
    pub const fn is_slave(self) -> bool {
        self.0 & Self::SHARED_MASK.0 == Self::SLAVE.0
    }

    /// `(flag & _periodic_mask) == _periodic_slave` (upstream `isPeriodicSlave`).
    #[inline]
    #[must_use]
    pub const fn is_periodic_slave(self) -> bool {
        self.0 & Self::PERIODIC_MASK.0 == Self::PERIODIC_SLAVE.0
    }

    /// `(flag & _periodic_mask) == _periodic_master` (upstream `isPeriodicMaster`).
    #[inline]
    #[must_use]
    pub const fn is_periodic_master(self) -> bool {
        self.0 & Self::PERIODIC_MASK.0 == Self::PERIODIC_MASTER.0
    }
}

impl BitAnd for NodeFlag {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl BitOr for NodeFlag {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitAndAssign for NodeFlag {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitOrAssign for NodeFlag {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl Not for NodeFlag {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}

impl fmt::Debug for NodeFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for NodeFlag {
    /// Upstream `operator<<(NodeFlag)`: `0x` followed by two hex digits.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:02x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_match_upstream() {
        assert_eq!(NodeFlag::MASTER.bits(), 0x03);
        assert_eq!(
            NodeFlag::PBC_SLAVE_DIST_GHOST,
            NodeFlag::PERIODIC_SLAVE | NodeFlag::PURE_GHOST
        );
        assert_eq!(
            NodeFlag::PBC_MASTER_DIST_MASTER,
            NodeFlag::PERIODIC_MASTER | NodeFlag::MASTER
        );
        assert_eq!(NodeFlag::default(), NodeFlag::NORMAL);
        assert_eq!(NodeFlag::SLAVE.to_string(), "0x05");
    }

    #[test]
    fn predicates() {
        let n = NodeFlag::NORMAL;
        assert!(n.is_local() && n.is_local_or_master() && !n.is_master());
        assert!(NodeFlag::MASTER.is_master() && NodeFlag::MASTER.is_local_or_master());
        assert!(NodeFlag::SLAVE.is_slave() && !NodeFlag::SLAVE.is_local_or_master());
        assert!(NodeFlag::PURE_GHOST.is_pure_ghost() && !NodeFlag::PURE_GHOST.is_local_or_master());
        let ps = NodeFlag::PBC_SLAVE_DIST_MASTER;
        assert!(ps.is_periodic_slave() && ps.is_master() && !ps.is_periodic_master());
        // A periodic master that is distributed-master is still local-or-master.
        assert!(NodeFlag::PBC_MASTER_DIST_MASTER.is_local_or_master());
        // A periodic slave that is local is *not* local-or-master (0x50 & 0xCC = 0x40).
        assert!(!NodeFlag::PERIODIC_SLAVE.is_local_or_master());
        let mut f = NodeFlag::NORMAL;
        f |= NodeFlag::PERIODIC_MASTER;
        f &= !NodeFlag::SHARED_MASK;
        assert!(f.is_periodic_master());
    }
}
