// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/contact_mechanics/contact_element.hh,
// src/common/aka_common.hh (ContactState, DetectionType).

//! Contact elements (CONT-017).
//!
//! `ContactState` (`_no_contact = 0`, `_stick = 1`, `_slip = 2`) and
//! `DetectionType` (`_explicit`, `_implicit`) are the shared enumerations of
//! `akantu_core::enums`, re-exported here.

pub use akantu_core::enums::{ContactState, DetectionType};
use akantu_core::{Element, Idx};

/// A slave node paired with the master facet it projects on (upstream
/// `ContactElement{slave, master}`). Its connectivity is
/// `[slave, master nodes…]` (upstream `constructConnectivity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContactElement {
    /// Slave node.
    pub slave: Idx,
    /// Master facet.
    pub master: Element,
}

impl ContactElement {
    /// New contact element.
    #[must_use]
    pub const fn new(slave: Idx, master: Element) -> Self {
        Self { slave, master }
    }

    /// Number of nodes: nodes of the master facet + 1 (upstream
    /// `getNbNodes`).
    #[must_use]
    pub fn nb_nodes(&self) -> usize {
        self.master.ty().nb_nodes_per_element() + 1
    }
}

/// Integer code of a contact state (upstream `ComputeIntFromEnum`, used by
/// the `contact_state` dump field): 0, 1, 2.
#[must_use]
pub const fn contact_state_code(state: ContactState) -> i32 {
    match state {
        ContactState::NoContact => 0,
        ContactState::Stick => 1,
        ContactState::Slip => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::{ElementType, GhostType};

    #[test]
    fn nodes_and_codes() {
        let c = ContactElement::new(
            3,
            Element::new(ElementType::Segment2, 1, GhostType::NotGhost),
        );
        assert_eq!(c.nb_nodes(), 3);
        let t = ContactElement::new(
            3,
            Element::new(ElementType::Triangle3, 0, GhostType::NotGhost),
        );
        assert_eq!(t.nb_nodes(), 4);
        assert_ne!(c, t);
        assert_eq!(contact_state_code(ContactState::NoContact), 0);
        assert_eq!(contact_state_code(ContactState::Stick), 1);
        assert_eq!(contact_state_code(ContactState::Slip), 2);
    }
}
