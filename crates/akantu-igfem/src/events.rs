// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/mesh_igfem_spherical_growing_gel.hh
// (NewIGFEMNodesEvent, NewIGFEMElementsEvent).

//! Payloads of the IGFEM mesh events (extensions of the standard
//! `NodesAdded` / `ElementsAdded` events, see
//! [`akantu_mesh::events::EventExtension`]).

use akantu_core::{Array, Element, ElementType, GhostType, Int};

/// Extension of a `NodesAdded` event (upstream `NewIGFEMNodesEvent`): the
/// intersector's `new_node_per_elem` table of the type whose edges produced
/// the nodes.
#[derive(Clone, Debug)]
pub struct NewIgfemNodes {
    /// Column 0: number of new nodes of the element; then pairs (node id,
    /// local edge).
    pub new_node_per_elem: Array<Int>,
    /// Element type of the intersected elements.
    pub ty: ElementType,
    /// Ghost type of the intersected elements.
    pub ghost: GhostType,
}

/// Extension of the `ElementsAdded` event of an IGFEM re-meshing (upstream
/// `NewIGFEMElementsEvent::old_elements`): `old_elements[i]` is the element
/// (in the numbering **before** the mutation) that new element `i` replaces.
#[derive(Clone, Debug)]
pub struct NewIgfemElements {
    /// Replaced elements, in the order of the new elements.
    pub old_elements: Vec<Element>,
}
