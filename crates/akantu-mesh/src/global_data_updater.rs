// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh_global_data_updater.hh
// (MeshGlobalDataUpdater), src/mesh/mesh.cc (Mesh::updateGlobalData,
// Mesh::registerGlobalDataUpdater).

//! The global-data update hook after topology changes (MESH-033,
//! PART-017).
//!
//! Upstream, code that creates nodes and elements (the cohesive inserter)
//! calls `Mesh::updateGlobalData(nodes_event, elements_event)` before
//! sending its events: a registered `MeshGlobalDataUpdater` (the cohesive
//! model's `CohesiveMeshGlobalDataUpdater`) renumbers global ids and returns
//! the **global** numbers of new nodes and elements, which decide whether
//! the events are sent; without an updater the mesh returns the sizes of
//! the local lists. The base class's own `updateData` returns `(0, 0)`.
//!
//! In the port the collective part is a stage of the staged mutation
//! (`docs/ARCHITECTURE.md`): [`crate::DistributedHook`] (stage 2) renumbers
//! global ids, and stage 3 reduces the per-event counts so that an event is
//! dispatched on every rank iff its global count is non-zero (serial: the
//! local list sizes, as upstream without an updater). This module keeps
//! upstream's interface for code ported one to one: [`update_global_data`]
//! is `Mesh::updateGlobalData` and [`MeshGlobalDataUpdater`] the
//! strategy, whose default `update_data` returns `(0, 0)` as upstream's base
//! class. It sends no event.

use akantu_core::{Element, Idx, Result};

/// Upstream `MeshGlobalDataUpdater`.
pub trait MeshGlobalDataUpdater {
    /// Upstream `updateData(nodes_event, elements_event)`: the global
    /// numbers of new nodes and elements (default `(0, 0)`, upstream's base
    /// class).
    ///
    /// # Errors
    /// Implementation-defined (a failed collective renumbering).
    fn update_data(
        &mut self,
        new_nodes: &[Idx],
        new_elements: &[Element],
    ) -> Result<(usize, usize)> {
        let _ = (new_nodes, new_elements);
        Ok((0, 0))
    }
}

/// Upstream `Mesh::updateGlobalData(nodes_event, elements_event)`: the
/// registered updater's result, else the sizes of the local lists. Sends no
/// event.
///
/// # Errors
/// Those of the updater.
pub fn update_global_data(
    updater: Option<&mut dyn MeshGlobalDataUpdater>,
    new_nodes: &[Idx],
    new_elements: &[Element],
) -> Result<(usize, usize)> {
    match updater {
        Some(u) => u.update_data(new_nodes, new_elements),
        None => Ok((new_nodes.len(), new_elements.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::{ElementType, GhostType};

    struct Base;
    impl MeshGlobalDataUpdater for Base {}

    struct Doubling;
    impl MeshGlobalDataUpdater for Doubling {
        fn update_data(&mut self, n: &[Idx], e: &[Element]) -> Result<(usize, usize)> {
            Ok((2 * n.len(), 2 * e.len()))
        }
    }

    #[test]
    fn defaults_follow_upstream() {
        let nodes = [4, 5, 6];
        let elements = [Element::new(ElementType::Segment2, 1, GhostType::NotGhost)];
        assert_eq!(update_global_data(None, &nodes, &elements).unwrap(), (3, 1));
        assert_eq!(
            update_global_data(Some(&mut Base), &nodes, &elements).unwrap(),
            (0, 0)
        );
        assert_eq!(
            update_global_data(Some(&mut Doubling), &nodes, &elements).unwrap(),
            (6, 2)
        );
    }
}
