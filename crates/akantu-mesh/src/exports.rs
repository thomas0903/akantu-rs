// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_mesh.cc, python/py_group_manager.cc
// (getNodes, getConnectivity, NodeGroup.getNodes, ElementGroup.getElements,
// getElementalData*/getDataReal return references to the mesh's arrays).
// Export protocol: akantu-rs design (decision records D-024, D-047;
// docs/ARCHITECTURE.md).

//! Zero-copy exports of a mesh domain's arrays to a foreign runtime
//! (NumPy, C views).
//!
//! Exporting an array needs exclusive access to it (`SharedArray::export`
//! takes `&mut`, which proves that no native element guard is alive), so it
//! goes through the domain's write lock: [`SharedMesh::with_exports`]. The
//! closure only gets [`ExportSlot`]s, which can create export leases and
//! query their attachment but give no element or size access: values change
//! through native mesh operations or through the foreign views themselves,
//! and topology only through [`SharedMesh::mutate`] / [`SharedMesh::setup`],
//! so the consumer protocol of the domain is never bypassed.

use akantu_core::array_shared::{ForeignRuntime, SharedArray};
use akantu_core::array_storage::ExportLease;
use akantu_core::{ElementType, Error, GhostType, Idx, Int, Real, Result, UInt};

use crate::shared::SharedMesh;
use crate::{Mesh, MeshPart};

/// One exportable mesh array, reachable only to export it.
#[derive(Debug)]
pub struct ExportSlot<'a, T> {
    array: &'a mut SharedArray<T>,
}

impl<T: Copy> ExportSlot<'_, T> {
    /// Export the current allocation to `runtime` (see
    /// [`SharedArray::export`]).
    pub fn export(&mut self, runtime: &ForeignRuntime) -> Result<ExportLease<T>> {
        self.array.export(runtime)
    }

    /// `true` iff `lease` still views this array's live values.
    #[must_use]
    pub fn is_attached(&self, lease: &ExportLease<T>) -> bool {
        self.array.is_attached(lease)
    }
}

impl<T> ExportSlot<'_, T> {
    /// Number of tuples (metadata).
    #[must_use]
    pub fn len(&self) -> usize {
        self.array.len()
    }

    /// `true` if there are no tuples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.array.is_empty()
    }

    /// Components per tuple (metadata).
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.array.nb_component()
    }
}

fn slot<T>(array: &mut SharedArray<T>) -> ExportSlot<'_, T> {
    ExportSlot { array }
}

/// Exclusive access to the exportable arrays of one mesh of a domain (the
/// mesh or its facet mesh), for the duration of one
/// [`SharedMesh::with_exports`] call.
#[derive(Debug)]
pub struct MeshExports<'a> {
    mesh: &'a mut Mesh,
}

macro_rules! data_slot {
    ($name:ident, $get:ident, $t:ty) => {
        #[doc = concat!("The `(ty, ghost)` block of the elemental `", stringify!($t), "` dataset `name`.")]
        pub fn $name(
            &mut self,
            name: &str,
            ty: ElementType,
            ghost: GhostType,
        ) -> Result<ExportSlot<'_, $t>> {
            Ok(slot(self.mesh.data.$get(name)?.shared_mut(ty, ghost)?))
        }
    };
}

impl MeshExports<'_> {
    /// The facet mesh (error if none) or the mesh itself.
    pub fn part(&mut self, part: MeshPart) -> Result<MeshExports<'_>> {
        let mesh =
            match part {
                MeshPart::Main => &mut *self.mesh,
                MeshPart::Facets => self.mesh.mesh_facets.as_deref_mut().ok_or_else(|| {
                    Error::not_found("facet mesh", "No facet mesh is defined yet!")
                })?,
            };
        Ok(MeshExports { mesh })
    }

    /// Node coordinates.
    pub fn nodes(&mut self) -> ExportSlot<'_, Real> {
        slot(&mut self.mesh.nodes)
    }

    /// Connectivity of `(ty, ghost)`.
    pub fn connectivity(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<ExportSlot<'_, Idx>> {
        Ok(slot(self.mesh.connectivities.shared_mut(ty, ghost)?))
    }

    /// Elements of `(ty, ghost)` of the element group `group`.
    pub fn group_elements(
        &mut self,
        group: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<ExportSlot<'_, Idx>> {
        let g = self.mesh.groups.element_group_mut(group)?;
        Ok(slot(g.into_group().elements.shared_mut(ty, ghost)?))
    }

    /// Nodes of the node group `group`.
    pub fn node_group_nodes(&mut self, group: &str) -> Result<ExportSlot<'_, Idx>> {
        Ok(slot(
            self.mesh.groups.node_group_mut(group)?.nodes_shared_mut(),
        ))
    }

    data_slot!(data_real, reals_mut, Real);
    data_slot!(data_int, ints_mut, Int);
    data_slot!(data_uint, uints_mut, UInt);
    data_slot!(data_bool, bools_mut, bool);

    /// Retire every **exported** topology array of the mesh (connectivities,
    /// group element and node lists): their foreign views become
    /// stale-but-safe (they keep their values, writes no longer reach the
    /// mesh), the arrays keep their values in fresh allocations. Used when a
    /// model starts consuming the mesh: from then on topology changes go
    /// through the consumers' event protocol only (compatibility adjustment
    /// of the Python bindings, UD-532). Needs native access to the exported
    /// arrays (a native section of their runtime).
    pub fn detach_topology(&mut self) -> Result<()> {
        fn detach<T: Clone>(a: &mut SharedArray<T>) -> Result<()> {
            if a.is_exported() {
                let values = a.read()?.clone();
                a.write()?.replace(values);
            }
            Ok(())
        }
        for g in GhostType::ALL {
            for (_, a) in self.mesh.connectivities.as_map_mut().iter_mut(g) {
                detach(a)?;
            }
        }
        self.mesh
            .groups
            .for_each_exportable_list(&mut |a| detach(a))
    }
}

impl SharedMesh {
    /// Run `f` with exclusive access to the exportable arrays of the domain
    /// (foreign views, D-047). Fails promptly — without waiting — while the
    /// domain is poisoned, mutated or read (a read guard is alive, e.g. a
    /// model operation is running and has called back into foreign code).
    pub fn with_exports<R>(&self, f: impl FnOnce(&mut MeshExports<'_>) -> Result<R>) -> Result<R> {
        if let Some(e) = self.state()?.poison_error() {
            return Err(e);
        }
        let mut g = self.try_write()?;
        f(&mut MeshExports { mesh: &mut g })
    }
}
