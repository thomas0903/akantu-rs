// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/mesh_igfem_spherical_growing_gel{.hh,_tmpl.hh}
// (MeshIgfemSphericalGrowingGel, NewIGFEMNodesEvent, NewIGFEMElementsEvent).
// This file is LGPL; it is compiled only with the feature `cgal` because it
// uses the sphere intersector of the GPL-3.0-or-later crate
// `akantu-geometry-cgal` (decision D-042): a build with this feature is
// GPL-3.0-or-later as a whole.

//! Mesh generation for spherical inclusions ("growing gel pockets",
//! IGFEM-003): upstream `MeshIgfemSphericalGrowingGel<dim>`.
//!
//! [`MeshIgfemSphericalGrowingGel::build_igfem_mesh_from_spheres`] runs, in
//! one staged mesh mutation ([`akantu_mesh::SharedMesh::mutate`]):
//!
//! 1. **intersection** (`computeMeshQueryListIntersectionPoint`): for each
//!    ghost type and each of `_triangle_3`, `_igfem_triangle_4`,
//!    `_igfem_triangle_5` (enum order), the edges of the parent triangles
//!    are intersected with the spheres (exact spherical kernel); the new
//!    points are appended to the nodes (`NodesAdded` with a
//!    [`NewIgfemNodes`] extension). Points found again by a later type are
//!    reused (only nodes created in this pass are candidates);
//! 2. **elements** (`buildIGFEMMesh`): every element with new nodes
//!    becomes an `_igfem_triangle_4` (one cut edge, re-oriented so that it
//!    is local edge `(1, 2)`) or `_igfem_triangle_5` (two cut edges, the
//!    isolated vertex first), every IGFEM element without new nodes
//!    becomes a `_triangle_3` again; the replaced elements are erased;
//! 3. **node clean-up** (`removeAdditionalNodes`): the enriched nodes of
//!    the previous pass are removed and the new ones renumbered after the
//!    standard nodes.
//!
//! Steps 1–2 are phase (i)
//! ([`MeshIgfemSphericalGrowingGel::build_igfem_elements_from_spheres`]),
//! step 3 phase (ii)
//! ([`MeshIgfemSphericalGrowingGel::remove_previous_enriched_nodes`]). A
//! mesh without consumers runs both in one mutation; the IGFEM model runs
//! them as two mutations (D-095 rule 3: the removal renumbers nodes that
//! the additions of phase (i) name, and the DOF manager refuses a removal
//! after an addition in one mutation).
//!
//! **Event order (observational difference, documented in UD-607).**
//! Upstream sends `ChangedElementsEvent`, `NewIGFEMElementsEvent` (with the
//! old elements), then `RemovedElementsEvent`, each handled synchronously
//! on the mesh state of that moment. akantu-rs dispatches every event of a
//! mutation after the commit on the final mesh (ARCHITECTURE), so the
//! element indices of an `ElementsAdded` event must be valid in the final
//! numbering: the port records `ElementsRemoved` (the replaced elements,
//! upstream's numbering) **before** `ElementsAdded` (the new elements,
//! final indices, with a [`NewIgfemElements`] extension listing the old
//! elements in upstream's order). `ChangedElementsEvent` has no consumer in
//! akantu-rs and is not recorded. A node removal that removes nothing and
//! keeps the numbering (first pass) is not recorded either (upstream sends
//! an identity `RemovedNodesEvent`).
//!
//! Upstream defects: UD-608 (C, bit-rot): `computeMeshQueryListIntersectionPoint`
//! sums the number of new points with a `StaticCommunicator` that no longer
//! exists — serial port; UD-609 (C): parallel node types
//! (`updateNodeType`, `buildSegmentConnectivityToNodeType`) use the removed
//! `DistributedSynchronizer` API — the port refuses more than one rank with
//! `Error::Unsupported` (parallel hook as interface).

use std::sync::Arc;

use akantu_core::{Array, Element, ElementType, Error, GhostType, Idx, Int, Real, Result, try_idx};
use akantu_geometry_cgal::{MeshSphereIntersector, SkSphere, SphereIntersectorOptions};
use akantu_mesh::{Mesh, MeshEvent, MeshPart, MeshTransaction};

pub use crate::events::{NewIgfemElements, NewIgfemNodes};

/// Element types handled by the intersection (upstream `ELEMENT_LIST`).
pub const ELEMENT_LIST: [ElementType; 3] = [
    ElementType::Triangle3,
    ElementType::IgfemTriangle4,
    ElementType::IgfemTriangle5,
];

/// Statistics of one re-meshing pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrowingGelPass {
    /// Intersection points appended to the mesh.
    pub new_nodes: usize,
    /// Elements created (and as many erased).
    pub new_elements: usize,
    /// Enriched nodes of the previous pass removed.
    pub removed_nodes: usize,
}

/// Upstream `MeshIgfemSphericalGrowingGel<dim>` (`dim = 2`).
#[derive(Debug)]
pub struct MeshIgfemSphericalGrowingGel {
    dim: usize,
    /// Number of standard (FEM) nodes (upstream `nb_nodes_fem`).
    nb_nodes_fem: usize,
    /// Enriched nodes before the last intersection (upstream
    /// `nb_enriched_nodes`).
    nb_enriched_nodes: usize,
    options: SphereIntersectorOptions,
    tolerance_on_node: Option<Real>,
    /// A phase (i) ran and its phase (ii) (the removal of the previous
    /// enriched nodes) did not run yet.
    pending_removal: bool,
}

impl MeshIgfemSphericalGrowingGel {
    /// Upstream constructor: `nb_nodes_fem = mesh.getNbNodes()`.
    ///
    /// # Errors
    /// A mesh of dimension other than 2 (upstream instantiates `dim = 2`).
    pub fn new(mesh: &Mesh) -> Result<Self> {
        if mesh.spatial_dimension() != 2 {
            return Err(Error::DimensionMismatch {
                context: "MeshIgfemSphericalGrowingGel: spatial dimension (upstream instantiates dim = 2)",
                expected: 2,
                got: mesh.spatial_dimension(),
            });
        }
        Ok(Self {
            dim: 2,
            nb_nodes_fem: mesh.nb_nodes(),
            nb_enriched_nodes: 0,
            options: SphereIntersectorOptions::default(),
            tolerance_on_node: None,
            pending_removal: false,
        })
    }

    /// Intersector options (defaults: the corrected v4 semantics of
    /// UD-410…412; [`SphereIntersectorOptions::upstream`] reproduces v5).
    pub fn set_intersector_options(&mut self, options: SphereIntersectorOptions) {
        self.options = options;
    }

    /// `setToleranceIntersectionOnNode` of every intersector (default
    /// `1e-10`, UD-411).
    pub fn set_tolerance_intersection_on_node(&mut self, tol: Real) {
        self.tolerance_on_node = Some(tol);
    }

    /// Upstream `init()`: `nb_nodes_fem = mesh.getNbNodes()`; every element
    /// type of the mesh dimension must be `_triangle_3` (or an IGFEM
    /// triangle type, which upstream would create) — "Not ready for mesh
    /// type" otherwise.
    ///
    /// # Errors
    /// Another element type of dimension 2, or a distributed mesh (UD-609).
    pub fn init(&mut self, mesh: &Mesh) -> Result<()> {
        check_serial(mesh)?;
        self.nb_nodes_fem = mesh.nb_nodes();
        self.nb_enriched_nodes = 0;
        self.pending_removal = false;
        for ghost in GhostType::ALL {
            for ty in mesh.element_types(akantu_core::TypeFilter {
                spatial_dimension: Some(self.dim),
                ghost_type: ghost,
                kind: None,
            }) {
                if !ELEMENT_LIST.contains(&ty) {
                    return Err(Error::UnsupportedElementType {
                        ty,
                        context: "MeshIgfemSphericalGrowingGel::init (Not ready for mesh type)",
                    });
                }
            }
        }
        Ok(())
    }

    /// Upstream `getNbStandardNodes()`.
    #[must_use]
    pub fn nb_standard_nodes(&self) -> usize {
        self.nb_nodes_fem
    }

    /// Upstream `getNbEnrichedNodes()` (enriched nodes before the last
    /// intersection).
    #[must_use]
    pub fn nb_enriched_nodes(&self) -> usize {
        self.nb_enriched_nodes
    }

    /// Spheres with radii multiplied by `inf_fact` (upstream
    /// `computeMeshQueryListIntersectionPoint(query_list, inf_fact)`:
    /// `squared_radius() * inf_fact * inf_fact`, exact).
    ///
    /// # Errors
    /// Non-finite factor.
    pub fn inflated(spheres: &[SkSphere], inf_fact: Real) -> Result<Vec<SkSphere>> {
        spheres
            .iter()
            .map(|s| {
                Ok(SkSphere {
                    center: s.center.clone(),
                    squared_radius: s.squared_radius.mul_f64(inf_fact)?.mul_f64(inf_fact)?,
                })
            })
            .collect()
    }

    /// Upstream `buildIGFEMMeshFromSpheres(query_list)` as the topology
    /// closure of a mesh mutation (see the module docs).
    ///
    /// # Errors
    /// Unsupported element type, distributed mesh, more than two new nodes in
    /// one element (UD-413), degenerate edges, transaction errors.
    pub fn build_igfem_mesh_from_spheres(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        spheres: &[SkSphere],
    ) -> Result<GrowingGelPass> {
        let mut pass = self.build_igfem_elements_from_spheres(tx, spheres)?;
        pass.removed_nodes = self.remove_previous_enriched_nodes(tx)?;
        // one mutation holds both phases: nothing is owed once it returns
        self.pending_removal = false;
        Ok(pass)
    }

    /// Phase (i) of [`MeshIgfemSphericalGrowingGel::build_igfem_mesh_from_spheres`]
    /// (upstream `computeMeshQueryListIntersectionPoint` and `buildIGFEMMesh`
    /// without its final `removeAdditionalNodes`): the intersection points
    /// appended (`NodesAdded`), the replaced elements erased in **one**
    /// `erase_elements` call, the new elements added (`ElementsAdded` with
    /// the old → new map, [`NewIgfemElements`]). The enriched nodes of the
    /// previous pass stay in the mesh (orphaned) until phase (ii),
    /// [`MeshIgfemSphericalGrowingGel::remove_previous_enriched_nodes`].
    ///
    /// A mesh with consumers (an IGFEM model) runs the two phases as two
    /// mutations (D-095 rule 3: the node removal renumbers the nodes the
    /// additions of phase (i) name); a mesh without consumers may run them
    /// in one ([`MeshIgfemSphericalGrowingGel::build_igfem_mesh_from_spheres`]).
    ///
    /// # Errors
    /// See [`MeshIgfemSphericalGrowingGel::build_igfem_mesh_from_spheres`].
    pub fn build_igfem_elements_from_spheres(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        spheres: &[SkSphere],
    ) -> Result<GrowingGelPass> {
        check_serial(tx.mesh())?;
        if self.pending_removal {
            return Err(Error::invalid(
                "MeshIgfemSphericalGrowingGel: the previous pass's enriched nodes were not removed \
                 (phase (ii), remove_previous_enriched_nodes, must follow every phase (i))",
            ));
        }
        let new_nodes = self.compute_mesh_query_list_intersection_point(tx, spheres)?;
        let mut pass = self.build_igfem_mesh(tx, &new_nodes)?;
        pass.new_nodes = new_nodes.iter().map(|t| t.nb_points).sum();
        self.pending_removal = true;
        Ok(pass)
    }

    /// Phase (ii) (upstream `removeAdditionalNodes`, the end of
    /// `buildIGFEMMesh`): the enriched nodes of the previous pass are
    /// removed and the nodes of the last phase (i) renumbered after the
    /// standard nodes (`NodesRemoved`, connectivities renumbered). Returns
    /// the number of removed nodes; records nothing when there is nothing to
    /// remove (first pass: upstream sends an identity `RemovedNodesEvent`,
    /// UD-607).
    ///
    /// The "removal owed" flag ([`MeshIgfemSphericalGrowingGel::has_pending_removal`])
    /// is **not** cleared here: the removal is only done once the mutation
    /// that holds this call has committed, so the caller clears it after the
    /// commit ([`MeshIgfemSphericalGrowingGel::clear_pending_removal`]). A
    /// mutation refused after this call (a capability check of a consumer)
    /// rolls the mesh back and leaves the flag set, and the next phase (ii)
    /// runs again.
    ///
    /// Unlike upstream, the previous enriched nodes are removed even when
    /// phase (i) created no node (an interface that left the mesh): upstream
    /// returns early and keeps them as orphaned nodes, which leave singular
    /// rows in a model's stiffness (UD-1200).
    ///
    /// # Errors
    /// Nodes removed behind the generator's back, a distributed mesh,
    /// transaction errors.
    pub fn remove_previous_enriched_nodes(
        &mut self,
        tx: &mut MeshTransaction<'_>,
    ) -> Result<usize> {
        check_serial(tx.mesh())?;
        if !self.pending_removal {
            return Ok(0);
        }
        self.remove_additional_nodes(tx)
    }

    /// `true` between a phase (i) and its phase (ii).
    #[must_use]
    pub fn has_pending_removal(&self) -> bool {
        self.pending_removal
    }

    /// Forget the pending phase (ii): after the mutation that ran
    /// [`MeshIgfemSphericalGrowingGel::remove_previous_enriched_nodes`] has
    /// committed, after a phase (i) whose mutation was rolled back (the mesh is back to the state before it, so its
    /// enriched nodes are those of the pass before), or after a phase (i)
    /// with no previous enriched nodes (nothing to remove; the IGFEM model
    /// records no empty mutation).
    pub fn clear_pending_removal(&mut self) {
        self.pending_removal = false;
    }

    /// Upstream `computeMeshQueryListIntersectionPoint(query_list)`:
    /// intersect, append the new points, record `NodesAdded`; returns the
    /// `new_node_per_elem` table of every `(type, ghost)`.
    fn compute_mesh_query_list_intersection_point(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        spheres: &[SkSphere],
    ) -> Result<Vec<TypeIntersection>> {
        let nb_nodes = tx.mesh().nb_nodes();
        self.nb_enriched_nodes = nb_nodes.checked_sub(self.nb_nodes_fem).ok_or_else(|| {
            Error::invalid(format!(
                "MeshIgfemSphericalGrowingGel: the mesh has {nb_nodes} nodes, fewer than the {} \
                 standard nodes recorded by init",
                self.nb_nodes_fem
            ))
        })?;
        let nb_old_nodes = Int::try_from(nb_nodes)
            .map_err(|_| Error::invalid("node count exceeds the node index type"))?;
        let mut out = Vec::new();
        for ghost in GhostType::ALL {
            for ty in ELEMENT_LIST {
                let mut inter = MeshSphereIntersector::new(tx.mesh(), ty, self.options)?;
                if let Some(t) = self.tolerance_on_node {
                    inter.set_tolerance_intersection_on_node(t);
                }
                inter.construct_data(tx.mesh(), ghost)?;
                inter.compute_mesh_query_list_intersection_point(
                    tx.mesh(),
                    spheres,
                    nb_old_nodes,
                )?;
                let points = inter.intersection_points().clone();
                let mut nodes = Vec::with_capacity(points.len());
                for p in points.tuples() {
                    nodes.push(tx.push_node(MeshPart::Main, p)?);
                }
                let table = inter.new_node_per_elem().clone();
                if !nodes.is_empty() {
                    tx.record(
                        MeshPart::Main,
                        MeshEvent::NodesAdded {
                            nodes,
                            old_nodes: None,
                            origin: "MeshIgfemSphericalGrowingGel".into(),
                            extension: Some(Arc::new(NewIgfemNodes {
                                new_node_per_elem: table.clone(),
                                ty,
                                ghost,
                            })),
                        },
                    );
                }
                out.push(TypeIntersection {
                    ty,
                    ghost,
                    nb_points: points.len(),
                    new_node_per_elem: table,
                });
            }
        }
        Ok(out)
    }

    /// Upstream `buildIGFEMMesh()` (see the module docs for the event
    /// order).
    fn build_igfem_mesh(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        tables: &[TypeIntersection],
    ) -> Result<GrowingGelPass> {
        // new elements (type, connectivity) and the elements they replace,
        // in upstream's order
        let mut new_elements: Vec<(ElementType, GhostType, Vec<Idx>)> = Vec::new();
        let mut old_elements: Vec<Element> = Vec::new();
        for t in tables {
            let mesh = tx.mesh();
            if mesh.nb_element(t.ty, t.ghost) == 0 {
                continue;
            }
            let conn = mesh.connectivity(t.ty, t.ghost)?;
            for nel in 0..t.new_node_per_elem.len() {
                let row = t.new_node_per_elem.tuple(nel);
                let c = conn.get_tuple(nel)?;
                if t.ty != ElementType::Triangle3 && row[0] == 0 {
                    // a former IGFEM triangle becomes a regular triangle
                    new_elements.push((ElementType::Triangle3, t.ghost, c[..3].to_vec()));
                    old_elements.push(Element::try_new(t.ty, nel, t.ghost)?);
                } else if row[0] != 0 {
                    let (ty, conn_new) = igfem_connectivity(c, row)?;
                    new_elements.push((ty, t.ghost, conn_new));
                    old_elements.push(Element::try_new(t.ty, nel, t.ghost)?);
                }
            }
        }
        let n = new_elements.len();
        if n > 0 {
            // the replaced elements first (final numbering for ElementsAdded)
            tx.erase_elements(MeshPart::Main, &old_elements)?;
            let mut added = Vec::with_capacity(n);
            for (ty, ghost, c) in &new_elements {
                added.push(tx.push_element(MeshPart::Main, *ty, *ghost, c)?);
            }
            tx.record(
                MeshPart::Main,
                MeshEvent::ElementsAdded {
                    elements: added,
                    constitutive_law: None,
                    origin: "MeshIgfemSphericalGrowingGel".into(),
                    extension: Some(Arc::new(NewIgfemElements { old_elements })),
                },
            );
        }
        Ok(GrowingGelPass {
            new_elements: n,
            ..GrowingGelPass::default()
        })
    }

    /// Upstream `removeAdditionalNodes()`: the enriched nodes of the
    /// previous pass are removed, the new ones numbered after the standard
    /// nodes. Returns the number of removed nodes.
    fn remove_additional_nodes(&mut self, tx: &mut MeshTransaction<'_>) -> Result<usize> {
        let total = tx.mesh().nb_nodes();
        let old_total = self.nb_nodes_fem + self.nb_enriched_nodes;
        let nb_new = total.checked_sub(old_total).ok_or_else(|| {
            Error::invalid("MeshIgfemSphericalGrowingGel: nodes were removed behind its back")
        })?;
        if nb_new == 0 && self.nb_enriched_nodes == 0 {
            return Ok(0);
        }
        if self.nb_enriched_nodes == 0 {
            // identity numbering, nothing removed (upstream still sends it)
            return Ok(0);
        }
        let mut numbering = Vec::with_capacity(total);
        for n in 0..self.nb_nodes_fem {
            numbering.push(try_idx(n)?);
        }
        numbering.extend(std::iter::repeat_n(-1, self.nb_enriched_nodes));
        for n in 0..nb_new {
            numbering.push(try_idx(self.nb_nodes_fem + n)?);
        }
        tx.remove_nodes(MeshPart::Main, &numbering)?;
        Ok(self.nb_enriched_nodes)
    }
}

/// `new_node_per_elem` of one `(type, ghost)`.
#[derive(Debug)]
struct TypeIntersection {
    ty: ElementType,
    ghost: GhostType,
    nb_points: usize,
    new_node_per_elem: Array<Int>,
}

fn check_serial(mesh: &Mesh) -> Result<()> {
    if mesh.is_distributed() || mesh.communicator().size() > 1 {
        return Err(Error::Unsupported(
            "IGFEM growing-gel mesh generation on more than one rank (UD-609: upstream's \
             parallel node types use the removed DistributedSynchronizer API)"
                .into(),
        ));
    }
    Ok(())
}

/// Upstream `buildIGFEMMesh` connectivity of an enriched element from the
/// parent connectivity `c` (first three nodes) and its `new_node_per_elem`
/// row `[n, node₁, edge₁, node₂, edge₂]` (edge `k` joins local nodes `k` and
/// `k + 1 mod 3`).
///
/// # Errors
/// An edge id outside `0..3`, an impossible edge pair, or more than two new
/// nodes (upstream `AKANTU_ERROR`).
pub fn igfem_connectivity(c: &[Idx], row: &[Int]) -> Result<(ElementType, Vec<Idx>)> {
    if c.len() < 3 || row.len() < 5 {
        return Err(Error::DimensionMismatch {
            context: "buildIGFEMMesh: connectivity / new_node_per_elem row",
            expected: 5,
            got: row.len(),
        });
    }
    let node = |k: usize| -> Result<Idx> {
        Idx::try_from(row[k]).map_err(|_| Error::invalid("new node id out of range"))
    };
    match row[0] {
        1 => {
            // the cut edge becomes local edge (1, 2)
            let (a, b, d) = match row[2] {
                0 => (c[2], c[0], c[1]),
                1 => (c[0], c[1], c[2]),
                2 => (c[1], c[2], c[0]),
                e => {
                    return Err(Error::invalid(format!(
                        "A triangle has only 3 segments not {e}"
                    )));
                }
            };
            Ok((ElementType::IgfemTriangle4, vec![a, b, d, node(1)?]))
        }
        2 => {
            let (s1, s2) = (row[2], row[4]);
            let (n1, n2) = (node(1)?, node(3)?);
            let v = match (s1, s2) {
                (0, 1) => vec![c[1], c[2], c[0], n2, n1],
                (0, 2) => vec![c[0], c[1], c[2], n1, n2],
                (1, 2) => vec![c[2], c[0], c[1], n2, n1],
                (1, 0) => vec![c[1], c[2], c[0], n1, n2],
                (2, 0) => vec![c[0], c[1], c[2], n2, n1],
                (2, 1) => vec![c[2], c[0], c[1], n1, n2],
                _ => {
                    return Err(Error::invalid(format!(
                        "A triangle has only 3 segments (0 to 2) not {s1}and{s2}"
                    )));
                }
            };
            Ok((ElementType::IgfemTriangle5, v))
        }
        n => Err(Error::invalid(format!(
            "IGFEM cannot add {n} nodes to triangles"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six two-cut cases put the isolated vertex first, the node of the
    /// cut edge (0, 1) third and the node of the edge (2, 0) fourth, and keep
    /// the orientation (cyclic permutations only).
    #[test]
    fn connectivity_cases() {
        let c = [10, 11, 12];
        // edges: 0 = (10, 11), 1 = (11, 12), 2 = (12, 10); nodes 20 (first
        // pair), 21 (second pair)
        let cases = [
            ((0, 1), 11),
            ((0, 2), 10),
            ((1, 2), 12),
            ((1, 0), 11),
            ((2, 0), 10),
            ((2, 1), 12),
        ];
        for ((s1, s2), isolated) in cases {
            let (ty, v) = igfem_connectivity(&c, &[2, 20, s1, 21, s2]).unwrap();
            assert_eq!(ty, ElementType::IgfemTriangle5);
            assert_eq!(v[0], isolated, "{s1} {s2}");
            // cyclic order of the parent nodes preserved
            let pos = |n: Idx| c.iter().position(|&x| x == n).unwrap();
            assert_eq!((pos(v[0]) + 1) % 3, pos(v[1]));
            assert_eq!((pos(v[1]) + 1) % 3, pos(v[2]));
            // node 3 on edge (v0, v1), node 4 on edge (v2, v0)
            let edge_of = |node: Idx| if node == 20 { s1 } else { s2 };
            let edge = |a: Idx, b: Idx| {
                let (pa, pb) = (pos(a), pos(b));
                if (pa + 1) % 3 == pb { pa } else { pb }
            };
            assert_eq!(edge_of(v[3]), Int::try_from(edge(v[0], v[1])).unwrap());
            assert_eq!(edge_of(v[4]), Int::try_from(edge(v[2], v[0])).unwrap());
        }
        for (e, first) in [(0, 12), (1, 10), (2, 11)] {
            let (ty, v) = igfem_connectivity(&c, &[1, 20, e, 0, 0]).unwrap();
            assert_eq!(ty, ElementType::IgfemTriangle4);
            assert_eq!(v[0], first);
            assert_eq!(v[3], 20);
        }
        assert!(igfem_connectivity(&c, &[1, 20, 3, 0, 0]).is_err());
        assert!(igfem_connectivity(&c, &[2, 20, 0, 21, 0]).is_err());
        assert!(igfem_connectivity(&c, &[3, 20, 0, 21, 1]).is_err());
    }
}
