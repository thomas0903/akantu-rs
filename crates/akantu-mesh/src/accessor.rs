// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh_accessor.hh.

//! Raw mutation access (upstream `MeshAccessor`, a friend class used by mesh
//! readers, distribution and the cohesive inserter).
//!
//! Mutations through the accessor bump the mesh revision but do **not**
//! produce events: callers that change the topology of a mesh already used by
//! models must build and dispatch the matching [`crate::MeshEvents`]
//! themselves (as upstream code sends events explicitly after using
//! `MeshAccessor`).

use akantu_core::array_shared::ArrayMut;
use akantu_core::{Array, ElementType, Error, GhostType, GlobalIdx, Idx, NodeFlag, Real, Result};

use crate::mesh::{BBox, Mesh};

/// Mutable access to mesh internals.
#[derive(Debug)]
pub struct MeshAccessor<'m> {
    mesh: &'m mut Mesh,
}

impl<'m> MeshAccessor<'m> {
    /// Start mutating `mesh` (bumps its revision once).
    pub fn new(mesh: &'m mut Mesh) -> Self {
        mesh.revision += 1;
        Self { mesh }
    }

    /// Append a node with flag `NORMAL`; returns its index.
    pub fn push_node(&mut self, x: &[Real]) -> Result<Idx> {
        let n = akantu_core::try_idx(self.mesh.nodes.len())?;
        self.mesh.nodes.write()?.try_push_tuple(x)?;
        self.mesh.nodes_flags.push_value(NodeFlag::NORMAL);
        if let Some(g) = &mut self.mesh.nodes_global_ids {
            g.push_value(GlobalIdx::from(n));
        }
        Ok(n)
    }

    /// Node coordinates (resizing must keep `nodes_flags` in sync: use
    /// [`MeshAccessor::resize_nodes`]). A guard of the exportable array.
    ///
    /// # Panics
    /// As [`Mesh::nodes`] (exported, no native section).
    #[track_caller]
    pub fn nodes_mut(&mut self) -> ArrayMut<'_, Real> {
        self.mesh.nodes.borrow_mut()
    }

    /// Resize nodes, flags and global ids together (checked contract):
    /// the node count must be representable as a local
    /// index (`Idx`, D-012) and `nb_nodes × dim` must not overflow; memory
    /// for all three arrays is reserved before any of them changes, so an
    /// error leaves the mesh unchanged.
    pub fn resize_nodes(&mut self, nb_nodes: usize) -> Result<()> {
        akantu_core::try_local_count(nb_nodes)?;
        let m = &mut *self.mesh;
        akantu_core::checked_len(nb_nodes, m.nodes.nb_component(), "mesh nodes")?;
        let grow = nb_nodes.saturating_sub(m.nodes.len());
        m.nodes.write()?.try_reserve(grow)?;
        m.nodes_flags.try_reserve(grow)?;
        if let Some(g) = &mut m.nodes_global_ids {
            g.try_reserve(grow)?;
        }
        m.nodes.write()?.try_resize(nb_nodes, 0.0)?;
        m.nodes_flags.try_resize(nb_nodes, NodeFlag::NORMAL)?;
        if let Some(g) = &mut m.nodes_global_ids {
            g.try_resize(nb_nodes, akantu_core::INVALID_GLOBAL_IDX)?;
        }
        Ok(())
    }

    /// Node flags.
    pub fn nodes_flags_mut(&mut self) -> &mut Array<NodeFlag> {
        &mut self.mesh.nodes_flags
    }

    /// Global ids (created on first access, initialised to local ids).
    pub fn nodes_global_ids_mut(&mut self) -> &mut Array<GlobalIdx> {
        let n = self.mesh.nodes.len();
        self.mesh.nodes_global_ids.get_or_insert_with(|| {
            Array::from_vec((0..n as GlobalIdx).collect(), 1).unwrap_or_default()
        })
    }

    /// Connectivity of `(ty, ghost)`, created empty if absent (upstream
    /// `getConnectivity(type, ghost)` on the accessor). A guard of the
    /// exportable block.
    ///
    /// # Panics
    /// As [`Mesh::nodes`] (exported, no native section).
    #[track_caller]
    pub fn connectivity_mut(&mut self, ty: ElementType, ghost: GhostType) -> ArrayMut<'_, Idx> {
        // upstream ElementTypeMapArray::alloc id: "<container id>:<type>[:ghost]"
        let id = format!(
            "{}:{ty}{}",
            self.mesh.connectivities.id(),
            if ghost == GhostType::Ghost {
                ":ghost"
            } else {
                ""
            }
        );
        self.mesh
            .connectivities
            .as_map_mut()
            .get_or_insert_with(ty, ghost, || {
                Array::with_id(ty.nb_nodes_per_element().max(1), id).into()
            })
            .borrow_mut()
    }

    /// Resize the connectivity of `(ty, ghost)` to `new_size` elements,
    /// creating it if absent; new entries are `-1` (upstream
    /// `resizeConnectivity`: `resize(new_size, Idx(-1))`, "use carefully":
    /// no event, the caller fills the new rows; MESH-016).
    ///
    /// # Errors
    /// `new_size` not representable as a local index count (D-012) or an
    /// allocation failure; nothing changes then.
    pub fn resize_connectivity(
        &mut self,
        new_size: usize,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        akantu_core::try_local_count(new_size)?;
        self.connectivity_mut(ty, ghost).try_resize(new_size, -1)
    }

    /// Upstream `MeshAccessor::makeReady` → `Mesh::makeReady` (MESH-016):
    /// the global node count becomes the local one, the bounding boxes are
    /// recomputed and node flags are extended with `NORMAL` to the node
    /// count (upstream also resets its node → element sets, which the port
    /// does not cache in the mesh: `utils::fill_nodes_to_elements`).
    ///
    /// # Errors
    /// A distributed mesh whose bounding-box reduction fails.
    pub fn make_ready(&mut self) -> Result<()> {
        let m = &mut *self.mesh;
        m.nb_global_nodes = m.nodes.len();
        m.nodes_flags.try_resize(m.nodes.len(), NodeFlag::NORMAL)?;
        m.compute_bounding_box()
    }

    /// Declare `slave` a periodic copy of `master` (upstream
    /// `addPeriodicSlave`; flags are updated with the periodic bits).
    pub fn add_periodic_pair(&mut self, slave: Idx, master: Idx) -> Result<()> {
        let n = self.mesh.nodes.len();
        let check = |i: Idx| -> Result<usize> {
            let u = akantu_core::try_to_usize(i)?;
            if u < n {
                Ok(u)
            } else {
                Err(akantu_core::Error::OutOfRange {
                    what: "node",
                    index: u,
                    len: n,
                })
            }
        };
        let (s, m) = (check(slave)?, check(master)?);
        if s == m {
            return Err(akantu_core::Error::invalid(
                "a node cannot be its own periodic master",
            ));
        }
        self.mesh.is_periodic = true;
        self.mesh.periodic_slave_master.insert(slave, master);
        self.mesh
            .periodic_master_slaves
            .entry(master)
            .or_default()
            .push(slave);
        let flags = self.mesh.nodes_flags.as_mut_slice();
        flags[s] = NodeFlag(
            (flags[s].bits() & !NodeFlag::PERIODIC_MASK.bits()) | NodeFlag::PERIODIC_SLAVE.bits(),
        );
        flags[m] = NodeFlag(
            (flags[m].bits() & !NodeFlag::PERIODIC_MASK.bits()) | NodeFlag::PERIODIC_MASTER.bits(),
        );
        Ok(())
    }

    /// Owner rank of a shared/ghost node (upstream `nodes_prank`).
    pub fn set_node_prank(&mut self, node: Idx, rank: akantu_core::Rank) {
        self.mesh.nodes_prank.insert(node, rank);
    }

    /// Ghost-exchange schemes (built by distribution, updated by mesh
    /// mutations, see `crate::mutation`).
    pub fn synchronizers_mut(&mut self) -> &mut crate::synchronizers::MeshSynchronizers {
        &mut self.mesh.synchronizers
    }

    /// Mark the mesh distributed.
    pub fn set_distributed(&mut self) {
        self.mesh.is_distributed = true;
    }

    /// Install the facet mesh.
    pub fn set_mesh_facets(&mut self, facets: Mesh) {
        let mut facets = facets;
        facets.is_mesh_facets = true;
        self.mesh.mesh_facets = Some(Box::new(facets));
    }

    /// Install bounding boxes computed by the caller, without communication
    /// (build phase): `local` bounds this rank's nodes, `global` the whole
    /// mesh. For a mesh that is not distributed both are the local box, as
    /// upstream `computeBoundingBox` reduces over the communicator only for
    /// distributed meshes; the mesh readers compute it while staging, so
    /// that committing a read involves no fallible collective.
    /// Both boxes must have `spatial_dimension` coordinates
    /// (`Error::DimensionMismatch` otherwise, nothing changed).
    pub fn set_bounding_boxes(&mut self, local: BBox, global: BBox) -> Result<()> {
        let dim = self.mesh.spatial_dimension;
        for b in [&local, &global] {
            if b.lower.len() != dim || b.upper.len() != dim {
                return Err(Error::DimensionMismatch {
                    context: "bounding box coordinates (spatial dimension)",
                    expected: dim,
                    got: b.lower.len().max(b.upper.len()),
                });
            }
        }
        self.mesh.bbox_local = local;
        self.mesh.bbox = global;
        Ok(())
    }

    /// Full mutable access for algorithms living in this crate's siblings
    /// (distribution, readers). Prefer the specific methods above.
    pub fn mesh_mut(&mut self) -> &mut Mesh {
        self.mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unrepresentable node count is rejected before any change
    /// (no huge allocation is attempted).
    #[test]
    fn resize_nodes_is_checked() {
        let mut m = Mesh::new(3, "m").unwrap();
        let mut a = MeshAccessor::new(&mut m);
        a.resize_nodes(4).unwrap();
        assert!(a.resize_nodes(akantu_core::MAX_LOCAL_COUNT + 1).is_err());
        assert!(a.resize_nodes(usize::MAX).is_err());
        let m = a.mesh_mut();
        assert_eq!((m.nb_nodes(), m.nodes_flags().len()), (4, 4));
        m.check_consistency().unwrap();
    }

    /// MESH-016: `resizeConnectivity` pads with -1; `makeReady` sets the
    /// global node count, the flags and the boxes (upstream
    /// `Mesh::makeReady`).
    #[test]
    fn resize_connectivity_and_make_ready() {
        let mut m = Mesh::new(2, "m").unwrap();
        let mut a = MeshAccessor::new(&mut m);
        a.resize_nodes(3).unwrap();
        a.nodes_mut()
            .as_mut_slice()
            .copy_from_slice(&[0.0, 0.0, 2.0, 0.0, 0.0, -1.0]);
        a.resize_connectivity(2, ElementType::Segment2, GhostType::NotGhost)
            .unwrap();
        assert_eq!(
            a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
                .as_slice(),
            &[-1, -1, -1, -1]
        );
        assert!(
            a.resize_connectivity(
                akantu_core::MAX_LOCAL_COUNT + 1,
                ElementType::Segment2,
                GhostType::NotGhost
            )
            .is_err()
        );
        a.make_ready().unwrap();
        assert_eq!(m.nb_global_nodes(), 3);
        assert_eq!(m.lower_bounds(), &[0.0, -1.0]);
        assert_eq!(m.upper_bounds(), &[2.0, 0.0]);
        assert_eq!(m.nodes_flags().len(), 3);
    }

    /// Nodes grown through `nodes_mut()`
    /// directly leave the flags short; `makeReady` brings them back in step
    /// (upstream `Mesh::makeReady` resizes `nodes_flags` to the node count,
    /// new flags `_normal`) and sets the global count and the boxes from
    /// every node.
    #[test]
    fn make_ready_resizes_the_flags_of_directly_grown_nodes() {
        let mut m = Mesh::new(2, "m").unwrap();
        let mut a = MeshAccessor::new(&mut m);
        a.push_node(&[0.0, 0.0]).unwrap();
        a.nodes_mut().push_tuple(&[1.0, 5.0]);
        a.nodes_mut().push_tuple(&[-3.0, 2.0]);
        assert_eq!(a.mesh_mut().nodes_flags().len(), 1, "flags not in step yet");
        a.make_ready().unwrap();
        assert_eq!(m.nodes_flags().len(), 3);
        assert!(
            m.nodes_flags()
                .as_slice()
                .iter()
                .all(|f| *f == NodeFlag::NORMAL)
        );
        assert_eq!(m.nb_global_nodes(), 3);
        assert_eq!(m.lower_bounds(), &[-3.0, 0.0]);
        assert_eq!(m.upper_bounds(), &[1.0, 5.0]);
        m.check_consistency().unwrap();
    }

    /// Boxes are installed without communication; a box of
    /// the wrong dimension is rejected before anything changes.
    #[test]
    fn set_bounding_boxes_is_checked() {
        let mut m = Mesh::new(2, "m").unwrap();
        let mut a = MeshAccessor::new(&mut m);
        let mut b = BBox::empty(2);
        b.add_point(&[1.0, -2.0]);
        b.add_point(&[3.0, 4.0]);
        assert!(a.set_bounding_boxes(BBox::empty(3), b.clone()).is_err());
        assert!(a.set_bounding_boxes(b.clone(), BBox::empty(1)).is_err());
        assert_eq!(a.mesh_mut().bbox(), &BBox::empty(2));
        a.set_bounding_boxes(b.clone(), b.clone()).unwrap();
        assert_eq!((m.bbox(), m.local_bbox()), (&b, &b));
    }
}
