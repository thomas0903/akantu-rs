// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/contact_mechanics/surface_selector.hh,
// surface_selector.cc, src/mesh/element_group.cc (append, optimize),
// src/mesh/element_group_inline_impl.hh (add).

//! Surface selectors (CONT-013): which nodes are masters and slaves, and
//! which facets form the `contact_surface`.
//!
//! Upstream selectors create the element group `contact_surface` **in the
//! mesh** (or in the facet mesh for the cohesive selectors) and return live
//! references to node groups. In the port the mesh is shared by its models
//! and may only change through `SharedMesh::mutate` (`docs/ARCHITECTURE.md`), so
//! a selector **owns** its [`ContactSurface`] — with exactly the content and
//! order upstream's group has after `optimize()` (facets sorted and unique
//! per type, nodes sorted and unique) — and resolves the master/slave node
//! groups by name in the mesh at each search (upstream's live references).
//! A second selector on the same mesh is therefore not an error (upstream:
//! "group already exists").
//!
//! **The mesh group.** Upstream's documented
//! behaviour — "by default the contact model creates a group named
//! contact_surface" (`examples/python/contact_mechanics_model/compression/
//! detection-explicit.py:36-37` reads it from the mesh) — is kept: the
//! surface is **mirrored** as the element group [`CONTACT_SURFACE_GROUP`]
//! (with its node group) of the selector's mesh part
//! ([`write_contact_surface_group`]), written through a group-only
//! `SharedMesh::mutate` when the selector is set
//! ([`crate::ContactMechanicsModel::set_surface_selector`] on a mesh
//! without consumers, the couplers' `set_surface_selector` otherwise) and
//! rewritten after each insertion forwarded to a cohesive selector (the
//! solid–cohesive–contact coupler).
//!
//! Observable differences of the selector-owned surface
//! (content and order are identical to upstream's group, verified by the
//! oracle replays):
//! * a mesh that already has a group named `contact_surface` makes the
//!   upstream physical selector throw (name collision); the port replaces
//!   it by the surface's mirror;
//! * two selectors on one mesh are accepted (upstream: the second physical
//!   selector throws "group already exists"), each with its own surface;
//!   the mesh group mirrors the last one set;
//! * the mirror is written when the selector is **set**, not when it is
//!   built (the port's selector is built from a read-only mesh), and a
//!   contact model inside a coupler writes it only through the coupler's
//!   `set_surface_selector` (it needs the models' consumer tokens);
//! * a contact model built beside a separately built model of the same
//!   mesh, without a coupler (Python: `SolidMechanicsModel(mesh)` and
//!   `ContactMechanicsModel(mesh)`), writes **no** group: the mesh has a
//!   consumer whose handler the contact model cannot pass to the group-only
//!   mutation. A later `getElementGroup("contact_surface")` fails loudly
//!   (not found); no result is silently wrong. Set
//!   the selector through a coupler, or before the other model is built;
//! * mesh events (cohesive insertion) reach the surface only through the
//!   model's [`crate::ContactMechanicsModel::on_cohesive_elements_added`]
//!   (upstream: the group is a mesh event handler of the mesh itself).

use akantu_core::{Element, ElementType, ElementTypeMap, Error, GhostType, Idx, Result, SliceRef};
use akantu_mesh::GroupManager;
use akantu_mesh::utils::SUBELEMENT_TO_ELEMENT;
use akantu_mesh::{Mesh, MeshPart};

/// Name of the element group that mirrors the contact surface (upstream
/// `createElementGroup("contact_surface", dim - 1)` of the selectors).
pub const CONTACT_SURFACE_GROUP: &str = "contact_surface";

/// Write `surface` as the element group [`CONTACT_SURFACE_GROUP`] of
/// `groups` (dimension `dimension`, replacing a group of that name) with
/// its node group `contact_surface_nodes`: the surface's facets in order
/// and its nodes, then `optimize` — upstream's group after the selectors'
/// `append` / `add` and `optimize`.
///
/// # Errors
/// The node group `contact_surface_nodes` is shared by another element
/// group (UD-125); an invalid element.
pub fn write_contact_surface_group(
    groups: &mut GroupManager,
    surface: &ContactSurface,
    dimension: usize,
) -> Result<()> {
    let mut g = groups.create_element_group_replace(CONTACT_SURFACE_GROUP, dimension, true)?;
    for ghost in GhostType::ALL {
        for ty in surface.element_types(ghost) {
            for &e in surface.elements_of(ty, ghost) {
                g.add(
                    Element::try_new(ty, akantu_core::try_to_usize(e)?, ghost)?,
                    None,
                    false,
                )?;
            }
        }
    }
    for &n in surface.nodes() {
        g.add_node(n, false);
    }
    g.optimize();
    Ok(())
}
use akantu_parser::{Parser, ParserType, SearchScope};

/// The facets and nodes of the contact surface (upstream element group
/// `contact_surface` with its node group).
#[derive(Clone, Debug, Default)]
pub struct ContactSurface {
    elements: ElementTypeMap<Vec<Idx>>,
    nodes: Vec<Idx>,
    optimized: bool,
}

impl ContactSurface {
    /// Empty surface.
    #[must_use]
    pub fn new() -> Self {
        Self {
            optimized: true,
            ..Self::default()
        }
    }

    /// Append an element and, with `nodes`, its nodes (upstream
    /// `ElementGroup::add(el, add_nodes = true, check_for_duplicate)`).
    pub fn add(&mut self, element: Element, nodes: Option<&[Idx]>) -> Result<()> {
        let e = element
            .try_idx()
            .ok_or_else(|| Error::invalid("cannot add ElementNull to the contact surface"))?;
        let e = akantu_core::try_idx(e)?;
        match self.elements.get_mut(element.ty(), element.ghost_type()) {
            Some(v) => v.push(e),
            None => {
                self.elements
                    .insert(element.ty(), element.ghost_type(), vec![e])?;
            }
        }
        if let Some(nodes) = nodes {
            self.nodes.extend_from_slice(nodes);
        }
        self.optimized = false;
        Ok(())
    }

    /// Append the elements and the nodes of the mesh group `name` (upstream
    /// `ElementGroup::append`).
    pub fn append_group(&mut self, mesh: &Mesh, name: &str) -> Result<()> {
        let g = mesh.groups().element_group(name)?;
        for ghost in GhostType::ALL {
            for ty in g.group().element_types(ghost) {
                let list = g.elements_of(ty, ghost);
                match self.elements.get_mut(ty, ghost) {
                    Some(v) => v.extend_from_slice(&list),
                    None => {
                        self.elements.insert(ty, ghost, list.to_vec())?;
                    }
                }
            }
        }
        self.nodes.extend_from_slice(g.nodes().as_slice());
        self.optimized = false;
        Ok(())
    }

    /// Sort and remove duplicates (upstream `ElementGroup::optimize`).
    pub fn optimize(&mut self) {
        for ghost in GhostType::ALL {
            for ty in ElementType::ALL {
                if let Some(v) = self.elements.get_mut(ty, ghost) {
                    v.sort_unstable();
                    v.dedup();
                }
            }
        }
        self.nodes.sort_unstable();
        self.nodes.dedup();
        self.optimized = true;
    }

    /// `true` if `(ty, ghost, index)` belongs to the surface (upstream
    /// `getElements(type, ghost).find(index) != -1`).
    #[must_use]
    pub fn contains(&self, ty: ElementType, ghost: GhostType, index: Idx) -> bool {
        match self.elements.get(ty, ghost) {
            None => false,
            Some(v) if self.optimized => v.binary_search(&index).is_ok(),
            Some(v) => v.contains(&index),
        }
    }

    /// Element types of the surface for `ghost`, enum order (upstream
    /// `group.elementTypes(dim, ghost)`).
    #[must_use]
    pub fn element_types(&self, ghost: GhostType) -> Vec<ElementType> {
        self.elements.iter(ghost).map(|(ty, _)| ty).collect()
    }

    /// Facets of `(ty, ghost)` in group order.
    #[must_use]
    pub fn elements_of(&self, ty: ElementType, ghost: GhostType) -> &[Idx] {
        self.elements.get(ty, ghost).map_or(&[], Vec::as_slice)
    }

    /// Nodes of the surface (sorted after [`ContactSurface::optimize`]).
    #[must_use]
    pub fn nodes(&self) -> &[Idx] {
        &self.nodes
    }

    /// Number of facets.
    #[must_use]
    pub fn nb_elements(&self) -> usize {
        GhostType::ALL
            .iter()
            .flat_map(|&g| self.elements.iter(g).map(|(_, v)| v.len()))
            .sum()
    }
}

/// Selects the master and slave nodes and the contact surface (upstream
/// abstract `SurfaceSelector`).
pub trait SurfaceSelector: std::fmt::Debug + Send {
    /// Which mesh of the domain the selector works on (`Facets` for the
    /// cohesive selectors, upstream `mesh.getMeshFacets()`).
    fn part(&self) -> MeshPart;

    /// Master nodes (upstream `getMasterList`), resolved in `mesh` (the
    /// selector's part).
    fn master_list<'a>(&'a self, mesh: &'a Mesh) -> Result<SliceRef<'a, Idx>>;

    /// Slave nodes (upstream `getSlaveList`).
    fn slave_list<'a>(&'a self, mesh: &'a Mesh) -> Result<SliceRef<'a, Idx>>;

    /// The facets of the contact surface (upstream group `contact_surface`).
    fn contact_surface(&self) -> &ContactSurface;

    /// New elements in the mesh (upstream `onElementsAdded`): the cohesive
    /// selectors add the two facets of every new cohesive element, read
    /// from `subelement_to_element` of `mesh_facets`. Default: nothing.
    fn on_elements_added(&mut self, mesh_facets: &Mesh, elements: &[Element]) -> Result<()> {
        let _ = (mesh_facets, elements);
        Ok(())
    }
}

/// `master` and `slave` group names of the first `contact_detector`
/// section (upstream dereferences the first section without checking that
/// one exists; here an error).
fn master_slave_names(parser: &Parser) -> Result<(String, String)> {
    let section = parser
        .root
        .sub_sections_of(ParserType::ContactDetector)
        .next()
        .ok_or_else(|| Error::not_found("section", "contact_detector"))?;
    let master = section.get_parameter_value::<String>("master", SearchScope::Current)?;
    let slave = section.get_parameter_value::<String>("slave", SearchScope::Current)?;
    Ok((master, slave))
}

/// Surface from physical names (upstream `PhysicalSurfaceSelector`): the
/// `master` and `slave` element groups named in the first
/// `contact_detector` section; `contact_surface` = master ∪ slave facets;
/// the master (slave) list is the node group of the master (slave) group.
#[derive(Clone, Debug)]
pub struct PhysicalSurfaceSelector {
    master: String,
    slave: String,
    surface: ContactSurface,
}

impl PhysicalSurfaceSelector {
    /// Upstream `PhysicalSurfaceSelector(mesh)` with the input file
    /// `parser`.
    ///
    /// # Errors
    /// No `contact_detector` section, missing `master`/`slave` parameters
    /// or groups.
    pub fn new(mesh: &Mesh, parser: &Parser) -> Result<Self> {
        let (master, slave) = master_slave_names(parser)?;
        Self::with_groups(mesh, &master, &slave)
    }

    /// Selector on explicit group names (Python / programmatic use).
    pub fn with_groups(mesh: &Mesh, master: &str, slave: &str) -> Result<Self> {
        let mut surface = ContactSurface::new();
        surface.append_group(mesh, master)?;
        surface.optimize();
        surface.append_group(mesh, slave)?;
        surface.optimize();
        surface.optimize();
        Ok(Self {
            master: master.to_owned(),
            slave: slave.to_owned(),
            surface,
        })
    }

    /// Master group name.
    #[must_use]
    pub fn master_name(&self) -> &str {
        &self.master
    }

    /// Slave group name.
    #[must_use]
    pub fn slave_name(&self) -> &str {
        &self.slave
    }
}

impl SurfaceSelector for PhysicalSurfaceSelector {
    fn part(&self) -> MeshPart {
        MeshPart::Main
    }
    fn master_list<'a>(&'a self, mesh: &'a Mesh) -> Result<SliceRef<'a, Idx>> {
        Ok(mesh
            .groups()
            .element_group(&self.master)?
            .nodes()
            .into_slice())
    }
    fn slave_list<'a>(&'a self, mesh: &'a Mesh) -> Result<SliceRef<'a, Idx>> {
        Ok(mesh
            .groups()
            .element_group(&self.slave)?
            .nodes()
            .into_slice())
    }
    fn contact_surface(&self) -> &ContactSurface {
        &self.surface
    }
}

/// Facets of the cohesive elements of `elements` (both sides) added to
/// `surface` with their nodes, then `optimize` (upstream
/// `CohesiveSurfaceSelector::onElementsAdded`).
fn add_cohesive_facets(
    surface: &mut ContactSurface,
    mesh_facets: &Mesh,
    elements: &[Element],
) -> Result<()> {
    for &el in elements {
        if el.kind() != akantu_core::ElementKind::Cohesive {
            continue;
        }
        let map = mesh_facets.data().element_map(SUBELEMENT_TO_ELEMENT)?;
        let row = map.get(el.ty(), el.ghost_type())?.get_tuple(el.idx())?;
        for &facet in row {
            let nodes = mesh_facets.element_connectivity(facet)?.to_vec();
            surface.add(facet, Some(&nodes))?;
        }
    }
    surface.optimize();
    Ok(())
}

/// Surface made of the facets of inserted cohesive elements (upstream
/// `CohesiveSurfaceSelector`, on the facet mesh): starts empty; master =
/// slave = the nodes of the surface.
#[derive(Clone, Debug, Default)]
pub struct CohesiveSurfaceSelector {
    surface: ContactSurface,
}

impl CohesiveSurfaceSelector {
    /// Empty cohesive surface.
    #[must_use]
    pub fn new() -> Self {
        Self {
            surface: ContactSurface::new(),
        }
    }
}

impl SurfaceSelector for CohesiveSurfaceSelector {
    fn part(&self) -> MeshPart {
        MeshPart::Facets
    }
    fn master_list<'a>(&'a self, _mesh: &'a Mesh) -> Result<SliceRef<'a, Idx>> {
        Ok(self.surface.nodes().into())
    }
    fn slave_list<'a>(&'a self, _mesh: &'a Mesh) -> Result<SliceRef<'a, Idx>> {
        Ok(self.surface.nodes().into())
    }
    fn contact_surface(&self) -> &ContactSurface {
        &self.surface
    }
    fn on_elements_added(&mut self, mesh_facets: &Mesh, elements: &[Element]) -> Result<()> {
        add_cohesive_facets(&mut self.surface, mesh_facets, elements)
    }
}

/// Physical master/slave groups of the facet mesh plus the facets of
/// inserted cohesive elements (upstream `AllSurfaceSelector`); master =
/// slave = the nodes of the surface.
#[derive(Clone, Debug)]
pub struct AllSurfaceSelector {
    master: String,
    slave: String,
    surface: ContactSurface,
}

impl AllSurfaceSelector {
    /// Upstream `AllSurfaceSelector(mesh)`: `mesh_facets` is the facet mesh
    /// (groups `master` and `slave` of the first `contact_detector` section
    /// must exist in it).
    pub fn new(mesh_facets: &Mesh, parser: &Parser) -> Result<Self> {
        let (master, slave) = master_slave_names(parser)?;
        let mut surface = ContactSurface::new();
        surface.append_group(mesh_facets, &master)?;
        surface.optimize();
        surface.append_group(mesh_facets, &slave)?;
        surface.optimize();
        surface.optimize();
        Ok(Self {
            master,
            slave,
            surface,
        })
    }

    /// Master group name.
    #[must_use]
    pub fn master_name(&self) -> &str {
        &self.master
    }

    /// Slave group name.
    #[must_use]
    pub fn slave_name(&self) -> &str {
        &self.slave
    }
}

impl SurfaceSelector for AllSurfaceSelector {
    fn part(&self) -> MeshPart {
        MeshPart::Facets
    }
    fn master_list<'a>(&'a self, _mesh: &'a Mesh) -> Result<SliceRef<'a, Idx>> {
        Ok(self.surface.nodes().into())
    }
    fn slave_list<'a>(&'a self, _mesh: &'a Mesh) -> Result<SliceRef<'a, Idx>> {
        Ok(self.surface.nodes().into())
    }
    fn contact_surface(&self) -> &ContactSurface {
        &self.surface
    }
    fn on_elements_added(&mut self, mesh_facets: &Mesh, elements: &[Element]) -> Result<()> {
        add_cohesive_facets(&mut self.surface, mesh_facets, elements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_optimize_and_membership() {
        let mut s = ContactSurface::new();
        let e = |i| Element::new(ElementType::Segment2, i, GhostType::NotGhost);
        s.add(e(5), Some(&[9, 3])).unwrap();
        s.add(e(1), Some(&[3, 4])).unwrap();
        s.add(e(5), None).unwrap();
        assert!(s.contains(ElementType::Segment2, GhostType::NotGhost, 5));
        s.optimize();
        assert_eq!(
            s.elements_of(ElementType::Segment2, GhostType::NotGhost),
            &[1, 5]
        );
        assert_eq!(s.nodes(), &[3, 4, 9]);
        assert!(s.contains(ElementType::Segment2, GhostType::NotGhost, 1));
        assert!(!s.contains(ElementType::Segment2, GhostType::Ghost, 1));
        assert!(!s.contains(ElementType::Triangle3, GhostType::NotGhost, 1));
        assert_eq!(
            s.element_types(GhostType::NotGhost),
            vec![ElementType::Segment2]
        );
        assert_eq!(s.nb_elements(), 2);
        assert!(s.add(Element::NULL, None).is_err());
    }
}
