// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/contact_mechanics/contact_detector.hh,
// contact_detector.cc, contact_detector_inline_impl.hh.

//! Contact detection (upstream `ContactDetector`, CONT-006…011).
//!
//! One search (upstream `search`):
//! 1. node → facet sets of the detection mesh (`fillNodesToElements(dim − 1)`);
//! 2. `min_dd`, `max_dd = max_bb` = smallest / largest in-radius of the
//!    facets around the master nodes, on the current positions;
//! 3. **global search**: master and slave bounding boxes, each enlarged by
//!    `max_bb`, intersected; the master nodes inside the intersection go
//!    into a grid of spacing `√2 · max_dd` centred on it (the shared
//!    [`akantu_core::spatial_grid::SpatialGrid`], D-048);
//! 4. **local search**: for every slave node inside the intersection, the
//!    closest master node over the 3^d neighbouring cells (`<=`: the last
//!    equal candidate in visiting order wins), skipping masters that share a
//!    regular facet with the slave;
//! 5. **contact elements**: orthogonal projection of the slave on the
//!    `contact_surface` facets around its master node
//!    ([`crate::geometry::orthogonal_projection`]) and the self-contact
//!    filter ([`ContactDetector::is_valid_self_contact`]); rejected pairs
//!    get zero gap/normal/tangents/projection.
//!
//! Only penetrating slaves produce contact elements (no search margin).
//!
//! Deviations: **UD-001** — the slave normal of the self-contact filter is
//! zero-initialised (upstream accumulates into an uninitialised vector);
//! **UD-363**, **UD-371**, **UD-372** — corrected Newton metric, normals
//! oriented on the current positions, triangle-aware projection validity
//! (switches [`ContactDetector::projection_metric`],
//! [`ContactDetector::normal_orientation`],
//! [`ContactDetector::projection_validity`]);
//! a cell id outside the grid's index range (a degenerate spacing, e.g.
//! master facets of zero in-radius, so that `max_dd` stays at
//! `numeric_limits::min()`) is an error (**UD-270**; upstream converts
//! `std::floor` to `Int`, undefined);
//! a missing `contact_detector` section or surface selector is an error
//! (upstream dereferences an end iterator / a null pointer). The node →
//! facet sets are cached per mesh revision (same content as upstream's
//! recomputation at every search).

use akantu_core::array_shared::ArrayMut;
use akantu_core::enums::DetectionType;
use akantu_core::spatial_grid::{NeighborhoodScratch, SpatialGrid};
use akantu_core::{Array, Element, ElementKind, Error, Id, Idx, Real, Result, try_to_usize};
use akantu_mesh::BBox;
use akantu_mesh::utils::{NodeToElements, associated_elements, fill_nodes_to_elements};
use akantu_parser::{Parser, ParserSection, ParserType, SearchScope};

use crate::eigen_ops::{cpp_max, cpp_min, dot, norm};
use crate::element::ContactElement;
use crate::geometry::{
    MAX_FACET_NODES, MeshView, NormalOrientation, OutwardCache, OutwardFrame, ProjectionMetric,
    ProjectionParameters, ProjectionValidity, normal_from_coords, orthogonal_projection_in,
};
use crate::selector::SurfaceSelector;

/// Rule for the cell size of the detection grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridSpacing {
    /// Upstream (default): `√2 ·` the largest in-radius of every facet
    /// around a master node, side facets of the master body included. With
    /// elongated facets next to the contact surface a cell holds many master
    /// nodes and the local search degenerates towards `O(slaves × masters)`.
    #[default]
    Upstream,
    /// `√2 ·` the largest in-radius of the **contact-surface** facets
    /// around the master nodes (`maximum_bounding_box` and the minimal
    /// distance are unchanged). The 3^d neighbouring cells of a slave cover
    /// a different region than with [`GridSpacing::Upstream`], so the
    /// closest master node found can differ when it lies farther than one
    /// cell; an opt-in for meshes whose side facets are much larger than the
    /// surface facets.
    ContactSurface,
}

/// Nodal output arrays of a search (the model's fields).
#[derive(Debug)]
pub struct SearchOutput<'a> {
    /// Signed gaps (1 component; upstream convention `−|s − x|`; the guard
    /// of the model's exportable array).
    pub gaps: ArrayMut<'a, Real>,
    /// Outward normals (`dim` components; exportable, as `gaps`).
    pub normals: ArrayMut<'a, Real>,
    /// Unit tangents (`dim · (dim − 1)` components, column-major `dim ×
    /// (dim − 1)` per node).
    pub tangents: &'a mut Array<Real>,
    /// Natural projections (`dim − 1` components).
    pub projections: &'a mut Array<Real>,
}

/// Contact detector.
#[derive(Debug)]
pub struct ContactDetector {
    id: Id,
    spatial_dimension: usize,
    detection_type: DetectionType,
    projection_tolerance: Real,
    max_iterations: i64,
    extension_tolerance: Real,
    max_dd: Real,
    /// Largest in-radius of the contact-surface facets around the master
    /// nodes ([`GridSpacing::ContactSurface`]).
    surface_dd: Real,
    min_dd: Real,
    max_bb: Real,
    positions: Array<Real>,
    surface_selector: Option<Box<dyn SurfaceSelector>>,
    contact_pairs: Vec<(Idx, Idx)>,
    /// Newton metric of the natural projection (UD-363 switch).
    pub projection_metric: ProjectionMetric,
    /// Configuration orienting the normals outward (UD-371 switch), in the
    /// projection and in the self-contact filter.
    pub normal_orientation: NormalOrientation,
    /// Rule preferring projections inside their facet (UD-372 switch).
    pub projection_validity: ProjectionValidity,
    /// Cell size of the detection grid (default upstream's).
    pub grid_spacing: GridSpacing,
    node_to_elements: Option<(u64, NodeToElements)>,
    /// Outward directions of the facets on the positions of the running
    /// [`ContactDetector::create_contact_elements`] (a new generation per
    /// call).
    outward: OutwardCache,
    /// Facets whose in-radius the running search has taken (per type and
    /// ghost type: the search number that last visited each facet).
    visited: Vec<(akantu_core::ElementType, akantu_core::GhostType, Vec<u32>)>,
    /// Number of the running maximal-detection-distance computation.
    visit_round: u32,
}

impl ContactDetector {
    /// Upstream `ContactDetector(mesh, positions, id)`: copies `positions`
    /// (`dim` components) and parses the **first** `contact_detector`
    /// section of `parser` (mandatory).
    ///
    /// # Errors
    /// No `contact_detector` section, a missing or malformed parameter, an
    /// unknown detection type, positions of the wrong dimension.
    pub fn with_positions(
        spatial_dimension: usize,
        positions: &Array<Real>,
        parser: &Parser,
        id: impl Into<Id>,
    ) -> Result<Self> {
        akantu_core::dim::check_dim(spatial_dimension)?;
        if positions.nb_component() != spatial_dimension {
            return Err(Error::DimensionMismatch {
                context: "ContactDetector: positions",
                expected: spatial_dimension,
                got: positions.nb_component(),
            });
        }
        let section = parser
            .root
            .sub_sections_of(ParserType::ContactDetector)
            .next()
            .ok_or_else(|| Error::not_found("section", "contact_detector"))?;
        let mut d = Self {
            id: id.into(),
            spatial_dimension,
            detection_type: DetectionType::Explicit,
            projection_tolerance: 0.0,
            max_iterations: 0,
            extension_tolerance: 0.0,
            max_dd: 0.0,
            surface_dd: 0.0,
            min_dd: 0.0,
            max_bb: 0.0,
            positions: positions.clone(),
            surface_selector: None,
            contact_pairs: Vec::new(),
            projection_metric: ProjectionMetric::default(),
            normal_orientation: NormalOrientation::default(),
            projection_validity: ProjectionValidity::default(),
            grid_spacing: GridSpacing::default(),
            node_to_elements: None,
            outward: OutwardCache::default(),
            visited: Vec::new(),
            visit_round: 0,
        };
        d.parse_section(section)?;
        Ok(d)
    }

    /// Upstream `parseSection`: `type` (`explicit` / `implicit`),
    /// `projection_tolerance`, `max_iterations`, `extension_tolerance`
    /// (all required; other parameters such as `master`/`slave` are read by
    /// the selectors).
    pub fn parse_section(&mut self, section: &ParserSection) -> Result<()> {
        let ty = section.get_parameter_value::<String>("type", SearchScope::Current)?;
        self.detection_type = match ty.as_str() {
            "implicit" => DetectionType::Implicit,
            "explicit" => DetectionType::Explicit,
            other => {
                return Err(Error::invalid(format!("Unknown detection type : {other}")));
            }
        };
        self.projection_tolerance =
            section.get_parameter_value::<Real>("projection_tolerance", SearchScope::Current)?;
        self.max_iterations =
            i64::from(section.get_parameter_value::<i32>("max_iterations", SearchScope::Current)?);
        self.extension_tolerance =
            section.get_parameter_value::<Real>("extension_tolerance", SearchScope::Current)?;
        Ok(())
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Detection type (upstream it only sets an unused `alpha`).
    #[must_use]
    pub fn detection_type(&self) -> DetectionType {
        self.detection_type
    }

    /// Newton tolerance of the natural projection.
    #[must_use]
    pub fn projection_tolerance(&self) -> Real {
        self.projection_tolerance
    }

    /// Newton iterations of the natural projection.
    #[must_use]
    pub fn max_iterations(&self) -> i64 {
        self.max_iterations
    }

    /// Extension tolerance of the reference element.
    #[must_use]
    pub fn extension_tolerance(&self) -> Real {
        self.extension_tolerance
    }

    /// Current positions (upstream `getPositions`; the coupler copies the
    /// solid's current positions here before each search).
    #[must_use]
    pub fn positions(&self) -> &Array<Real> {
        &self.positions
    }

    /// Mutable current positions.
    pub fn positions_mut(&mut self) -> &mut Array<Real> {
        &mut self.positions
    }

    /// Replace the positions (upstream `setPositions`).
    pub fn set_positions(&mut self, positions: &Array<Real>) -> Result<()> {
        if positions.nb_component() != self.spatial_dimension {
            return Err(Error::DimensionMismatch {
                context: "ContactDetector::set_positions",
                expected: self.spatial_dimension,
                got: positions.nb_component(),
            });
        }
        self.positions = positions.clone();
        Ok(())
    }

    /// Set the surface selector (upstream `setSurfaceSelector`; mandatory
    /// before a search).
    pub fn set_surface_selector(&mut self, selector: Box<dyn SurfaceSelector>) {
        self.surface_selector = Some(selector);
    }

    /// The surface selector, if set.
    #[must_use]
    pub fn surface_selector(&self) -> Option<&dyn SurfaceSelector> {
        self.surface_selector.as_deref()
    }

    /// Mutable surface selector (mesh events of the cohesive selectors).
    pub fn surface_selector_mut(&mut self) -> Option<&mut (dyn SurfaceSelector + 'static)> {
        self.surface_selector.as_deref_mut()
    }

    fn selector(&self) -> Result<&dyn SurfaceSelector> {
        self.surface_selector
            .as_deref()
            .ok_or_else(|| Error::invalid("ContactDetector: no surface selector set"))
    }

    /// Largest facet in-radius around the master nodes (grid spacing / √2).
    #[must_use]
    pub fn maximum_detection_distance(&self) -> Real {
        self.max_dd
    }

    /// Smallest facet in-radius around the master nodes.
    #[must_use]
    pub fn minimum_detection_distance(&self) -> Real {
        self.min_dd
    }

    /// Bounding-box extension.
    #[must_use]
    pub fn maximum_bounding_box(&self) -> Real {
        self.max_bb
    }

    /// Upstream `setMaximumDetectionDistance` (overwritten by the next
    /// search, as upstream).
    pub fn set_maximum_detection_distance(&mut self, v: Real) {
        self.max_dd = v;
    }

    /// Upstream `setMinimumDetectionDistance`.
    pub fn set_minimum_detection_distance(&mut self, v: Real) {
        self.min_dd = v;
    }

    /// Upstream `setMaximumBoundingBox`.
    pub fn set_maximum_bounding_box(&mut self, v: Real) {
        self.max_bb = v;
    }

    /// Projection parameters of the detector.
    #[must_use]
    pub fn projection_parameters(&self) -> ProjectionParameters {
        ProjectionParameters {
            max_iterations: self.max_iterations,
            projection_tolerance: self.projection_tolerance,
            extension_tolerance: self.extension_tolerance,
            metric: self.projection_metric,
            orientation: self.normal_orientation,
            validity: self.projection_validity,
        }
    }

    /// Check the shape of the positions against the mesh.
    fn check_positions(&self, view: &MeshView<'_>) -> Result<()> {
        if view.dim() != self.spatial_dimension
            || self.positions.nb_component() != self.spatial_dimension
            || self.positions.len() != view.mesh.nb_nodes()
        {
            return Err(Error::DimensionMismatch {
                context: "ContactDetector: positions (nodes of the detection mesh)",
                expected: view.mesh.nb_nodes(),
                got: self.positions.len(),
            });
        }
        Ok(())
    }

    /// Node → facet sets (cached per mesh revision).
    fn node_to_elements(&mut self, view: &MeshView<'_>) -> Result<()> {
        let rev = view.mesh.revision();
        let fresh = matches!(&self.node_to_elements, Some((r, m)) if *r == rev && m.nb_rows() == view.mesh.nb_nodes());
        if !fresh {
            let m = fill_nodes_to_elements(view.mesh, Some(self.spatial_dimension - 1))?;
            self.node_to_elements = Some((rev, m));
        }
        Ok(())
    }

    /// Facets around `node` (the map is filled by [`ContactDetector::search`]).
    fn facets_of(&self, node: Idx) -> Result<&[Element]> {
        let (_, map) = self
            .node_to_elements
            .as_ref()
            .ok_or_else(|| Error::invalid("node → facet map not built"))?;
        associated_elements(map, node)
    }

    /// Position of `node`.
    fn pos(&self, node: Idx) -> Result<&[Real]> {
        self.positions.get_tuple(try_to_usize(node)?)
    }

    /// Upstream `computeMaximalDetectionDistance` (std::max/std::min
    /// semantics, starting from `numeric_limits::min()` / `max()`).
    ///
    /// A facet around several master nodes is measured once: `std::max` /
    /// `std::min` from those starts never keep a NaN and are idempotent, so
    /// the result does not depend on repetitions or order.
    fn compute_maximal_detection_distance(&mut self, view: &MeshView<'_>) -> Result<()> {
        let dim = self.spatial_dimension;
        let mut max_size = Real::MIN_POSITIVE;
        let mut min_size = Real::MAX;
        let mut surface_size = Real::MIN_POSITIVE;
        let by_surface = self.grid_spacing == GridSpacing::ContactSurface;
        self.visit_round = self.visit_round.wrapping_add(1);
        if self.visit_round == 0 {
            for t in &mut self.visited {
                t.2.fill(0);
            }
            self.visit_round = 1;
        }
        let round = self.visit_round;
        let mut visited = std::mem::take(&mut self.visited);
        let selector = self.selector()?;
        let masters = selector.master_list(view.mesh)?;
        let mut coords = [0.0; 3 * 27];
        for &master in &masters {
            for &element in self.facets_of(master)? {
                let pos = match visited
                    .iter()
                    .position(|t| t.0 == element.ty() && t.1 == element.ghost_type())
                {
                    Some(p) => p,
                    None => {
                        visited.push((element.ty(), element.ghost_type(), Vec::new()));
                        visited.len() - 1
                    }
                };
                let stamps = &mut visited[pos].2;
                let i = element.idx();
                if stamps.len() <= i {
                    stamps.resize(i + 1, 0);
                }
                if stamps[i] == round {
                    continue;
                }
                stamps[i] = round;
                let conn = view.mesh.element_connectivity(element)?;
                let nn = conn.len();
                if nn * dim > coords.len() {
                    return Err(Error::UnsupportedElementType {
                        ty: element.ty(),
                        context: "contact detection: element around a master node",
                    });
                }
                for (k, &n) in conn.iter().enumerate() {
                    coords[k * dim..(k + 1) * dim].copy_from_slice(self.pos(n)?);
                }
                let size =
                    akantu_fe::engine::element_inradius_of(element.ty(), &coords[..nn * dim]);
                max_size = cpp_max(max_size, size);
                min_size = cpp_min(min_size, size);
                if by_surface
                    && selector.contact_surface().contains(
                        element.ty(),
                        element.ghost_type(),
                        element.index(),
                    )
                {
                    surface_size = cpp_max(surface_size, size);
                }
            }
        }
        // the master list borrows `self` (through the selector): end it
        // before the fields are written (explicit guard `Drop`)
        drop(masters);
        self.visited = visited;
        self.min_dd = min_size;
        self.max_dd = max_size;
        self.surface_dd = surface_size;
        self.max_bb = max_size;
        Ok(())
    }

    /// Bounding box of `nodes` enlarged by `max_bb` (upstream
    /// `constructBoundingBox`).
    fn bounding_box(&self, nodes: &[Idx]) -> Result<BBox> {
        let mut bbox = BBox::empty(self.spatial_dimension);
        for &n in nodes {
            bbox.add_point(self.pos(n)?);
        }
        for l in &mut bbox.lower {
            *l -= self.max_bb;
        }
        for u in &mut bbox.upper {
            *u += self.max_bb;
        }
        Ok(bbox)
    }

    /// Upstream `globalSearch`: intersection of the enlarged master and
    /// slave boxes and the grid of the master nodes inside it.
    pub(crate) fn global_search(&self, view: &MeshView<'_>) -> Result<(BBox, SpatialGrid<Idx>)> {
        let selector = self.selector()?;
        let masters = selector.master_list(view.mesh)?;
        let slaves = selector.slave_list(view.mesh)?;
        let bbox_master = self.bounding_box(&masters)?;
        let bbox_slave = self.bounding_box(&slaves)?;
        let intersection = bbox_master.intersection(&bbox_slave);
        let center = intersection.center();
        let dd = match self.grid_spacing {
            GridSpacing::Upstream => self.max_dd,
            GridSpacing::ContactSurface => self.surface_dd,
        };
        let spacing = vec![2.0_f64.sqrt() * dd; self.spatial_dimension];
        let mut grid = SpatialGrid::with_spacing(self.spatial_dimension, &spacing, &center)?;
        for &n in &masters {
            let p = self.pos(n)?;
            if intersection.contains(p) {
                grid.insert(n, p)?;
            }
        }
        Ok((intersection, grid))
    }

    /// Upstream `localSearch`: (slave, closest master node) pairs.
    pub(crate) fn local_search(
        &mut self,
        view: &MeshView<'_>,
        intersection: &BBox,
        grid: &SpatialGrid<Idx>,
    ) -> Result<()> {
        let mut pairs = std::mem::take(&mut self.contact_pairs);
        pairs.clear();
        let result = self.local_search_into(view, intersection, grid, &mut pairs);
        self.contact_pairs = pairs;
        result
    }

    fn local_search_into(
        &self,
        view: &MeshView<'_>,
        intersection: &BBox,
        grid: &SpatialGrid<Idx>,
        pairs: &mut Vec<(Idx, Idx)>,
    ) -> Result<()> {
        let dim = self.spatial_dimension;
        let selector = self.selector()?;
        let slaves = selector.slave_list(view.mesh)?;
        // nodes sharing a regular facet with the current slave (upstream
        // re-scans the connectivities for every candidate: same predicate)
        let mut neighbours: Vec<Idx> = Vec::new();
        // cell-id buffers of the neighbourhood walk, reused for every slave
        let mut scratch = NeighborhoodScratch::default();
        for &slave in &slaves {
            let ps = self.pos(slave)?;
            if !intersection.contains(ps) {
                continue;
            }
            neighbours.clear();
            for &e in self.facets_of(slave)? {
                if e.kind() != ElementKind::Regular {
                    continue;
                }
                neighbours.extend_from_slice(&view.mesh.element_connectivity(e)?);
            }
            let mut closest_distance = Real::MAX;
            let mut closest: Option<Idx> = None;
            // upstream `getCellID(pos_slave).neighbors()` + `getCell`: the
            // cells first coordinate fastest, each in insertion order
            for neighbor_cell in grid.neighborhood_at(ps, &mut scratch)? {
                for &master in neighbor_cell {
                    if master == slave || neighbours.contains(&master) {
                        continue;
                    }
                    let pm = self.pos(master)?;
                    let mut d = [0.0; 3];
                    for i in 0..dim {
                        d[i] = ps[i] - pm[i];
                    }
                    let distance = norm(&d[..dim]);
                    if distance <= closest_distance {
                        closest = Some(master);
                        closest_distance = distance;
                    }
                }
            }
            if let Some(m) = closest {
                pairs.push((slave, m));
            }
        }
        Ok(())
    }

    /// Upstream `isValidSelfContact(slave, gap, normal)` for the pair
    /// `(slave, master_node)`: rejected if the master node is a node of a
    /// regular facet around the slave, if `|gap| > 2 min_dd`, or if the
    /// normalised sum of the outward normals of the slave's regular facets
    /// (oriented on the detector's [`NormalOrientation`], UD-371) is not
    /// opposite to `normal` (`n̄ · n ≥ 0`; NaN accepts). The sum starts from
    /// zero (**UD-001**).
    pub fn is_valid_self_contact(
        &self,
        view: &MeshView<'_>,
        slave: Idx,
        master_node: Idx,
        gap: Real,
        normal: &[Real],
    ) -> Result<bool> {
        let frame = OutwardFrame::of(self.normal_orientation, &self.positions);
        self.is_valid_self_contact_in(view, slave, master_node, gap, normal, frame)
    }

    /// [`ContactDetector::is_valid_self_contact`] with the slave normals
    /// oriented on `frame` (the detector's orientation, possibly cached).
    fn is_valid_self_contact_in(
        &self,
        view: &MeshView<'_>,
        slave: Idx,
        master_node: Idx,
        gap: Real,
        normal: &[Real],
        frame: OutwardFrame<'_>,
    ) -> Result<bool> {
        let dim = self.spatial_dimension;
        let mut slave_normal = [0.0; 3];
        let mut coords = [0.0; 3 * MAX_FACET_NODES];
        for &element in self.facets_of(slave)? {
            if element.kind() != ElementKind::Regular {
                continue;
            }
            let conn = view.mesh.element_connectivity(element)?;
            if conn.len() * dim > coords.len() {
                return Err(Error::UnsupportedElementType {
                    ty: element.ty(),
                    context: "self-contact check: facet around a slave node",
                });
            }
            for (k, &n) in conn.iter().enumerate() {
                coords[k * dim..(k + 1) * dim].copy_from_slice(self.pos(n)?);
            }
            let n = normal_from_coords(view, &coords[..conn.len() * dim], element, Some(frame))?;
            for i in 0..dim {
                slave_normal[i] += n[i];
            }
            if conn.contains(&master_node) {
                return Ok(false);
            }
        }
        if gap.abs() > 2.0 * self.min_dd {
            return Ok(false);
        }
        let nrm = norm(&slave_normal[..dim]);
        if nrm != 0.0 {
            for v in slave_normal[..dim].iter_mut() {
                *v /= nrm;
            }
        }
        let product = dot(&slave_normal[..dim], &normal[..dim]);
        // upstream `not (product >= 0)`: a NaN product accepts the pair
        Ok(!matches!(
            product.partial_cmp(&0.0),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        ))
    }

    /// Upstream `createContactElements`: projection and filtering of every
    /// pair; contact elements are appended to `elements` in pair order.
    pub fn create_contact_elements(
        &mut self,
        view: &MeshView<'_>,
        elements: &mut Vec<ContactElement>,
        out: &mut SearchOutput<'_>,
    ) -> Result<()> {
        let dim = self.spatial_dimension;
        let sd = dim - 1;
        let pairs = std::mem::take(&mut self.contact_pairs);
        let params = self.projection_parameters();
        // the positions do not change during the call: one generation of
        // memoised outward directions
        let mut outward = std::mem::take(&mut self.outward);
        outward.next_generation();
        let outward = std::cell::RefCell::new(outward);
        let frame = match self.normal_orientation {
            NormalOrientation::Current => OutwardFrame::CurrentCached(&self.positions, &outward),
            NormalOrientation::Reference => OutwardFrame::Reference,
        };
        let result = (|| -> Result<()> {
            let surface = self.selector()?.contact_surface();
            for &(slave, master_node) in &pairs {
                let s = try_to_usize(slave)?;
                let mut slave_pos = [0.0; 3];
                slave_pos[..dim].copy_from_slice(self.pos(slave)?);
                let facets = self.facets_of(master_node)?;
                let found = orthogonal_projection_in(
                    view,
                    &self.positions,
                    &slave_pos[..dim],
                    facets,
                    surface,
                    &params,
                    frame,
                )?;
                if let Some(p) = &found {
                    out.gaps.get_tuple_mut(s)?[0] = p.gap;
                    out.projections
                        .get_tuple_mut(s)?
                        .copy_from_slice(&p.xi[..sd]);
                    out.normals
                        .get_tuple_mut(s)?
                        .copy_from_slice(&p.normal[..dim]);
                    out.tangents
                        .get_tuple_mut(s)?
                        .copy_from_slice(&p.tangents[..dim * sd]);
                }
                let accepted = match &found {
                    None => false,
                    Some(p) => self.is_valid_self_contact_in(
                        view,
                        slave,
                        master_node,
                        p.gap,
                        &p.normal[..dim],
                        frame,
                    )?,
                };
                if !accepted {
                    out.gaps.get_tuple_mut(s)?[0] = 0.0;
                    out.normals.get_tuple_mut(s)?.fill(0.0);
                    out.projections.get_tuple_mut(s)?.fill(0.0);
                    out.tangents.get_tuple_mut(s)?.fill(0.0);
                    continue;
                }
                if let Some(p) = found {
                    elements.push(ContactElement::new(slave, p.element));
                }
            }
            Ok(())
        })();
        self.outward = outward.into_inner();
        self.contact_pairs = pairs;
        self.contact_pairs.clear();
        result
    }

    /// Check the shapes of the output arrays.
    fn check_output(&self, view: &MeshView<'_>, out: &SearchOutput<'_>) -> Result<()> {
        let dim = self.spatial_dimension;
        let nb = view.mesh.nb_nodes();
        for (a, nbc) in [
            (&*out.gaps, 1),
            (&*out.normals, dim),
            (&*out.tangents, dim * (dim - 1)),
            (&*out.projections, dim - 1),
        ] {
            if a.len() != nb || a.nb_component() != nbc {
                return Err(Error::DimensionMismatch {
                    context: "ContactDetector::search: output array",
                    expected: nb * nbc,
                    got: a.len() * a.nb_component(),
                });
            }
        }
        Ok(())
    }

    /// Upstream `search(elements, gaps, normals, tangents, projections)`:
    /// all search steps; new contact elements are appended to `elements`,
    /// the slave rows of the output arrays are written (other rows are left
    /// untouched).
    pub fn search(
        &mut self,
        view: &MeshView<'_>,
        elements: &mut Vec<ContactElement>,
        out: &mut SearchOutput<'_>,
    ) -> Result<()> {
        if !(2..=3).contains(&self.spatial_dimension) {
            return Err(Error::Unsupported(format!(
                "contact detection in dimension {} (2 and 3 are supported)",
                self.spatial_dimension
            )));
        }
        self.check_positions(view)?;
        self.check_output(view, out)?;
        self.selector()?;
        self.node_to_elements(view)?;
        self.compute_maximal_detection_distance(view)?;
        self.contact_pairs.clear();
        let (intersection, grid) = self.global_search(view)?;
        self.local_search(view, &intersection, &grid)?;
        self.create_contact_elements(view, elements, out)
    }

    /// Pairs found by the last local search (cleared by
    /// [`ContactDetector::create_contact_elements`], as upstream).
    #[must_use]
    pub fn contact_pairs(&self) -> &[(Idx, Idx)] {
        &self.contact_pairs
    }
}

#[cfg(test)]
mod tests {
    use akantu_core::{ElementType, GhostType};
    use akantu_mesh::{Mesh, MeshAccessor};

    use super::*;
    use crate::selector::PhysicalSurfaceSelector;

    /// 2-D mesh of segments (`master` / `slave` groups), the detector on
    /// its positions, the global and local searches: returns the pairs and
    /// the cell (relative to the slave node 0's cell) of every node.
    fn pairs_and_cells(
        nodes: &[[Real; 2]],
        master: &[[Idx; 2]],
        slave: &[[Idx; 2]],
    ) -> (Vec<(Idx, Idx)>, Vec<[Idx; 2]>) {
        let mut mesh = Mesh::new(2, "tie").unwrap();
        let ty = ElementType::Segment2;
        let g = GhostType::NotGhost;
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for p in nodes {
                a.push_node(p).unwrap();
            }
            for c in master.iter().chain(slave) {
                a.connectivity_mut(ty, g).push_tuple(c);
            }
        }
        for (name, first, n) in [
            ("master", 0, master.len()),
            ("slave", master.len(), slave.len()),
        ] {
            mesh.groups_mut().create_element_group(name, 1).unwrap();
            for i in first..first + n {
                mesh.element_group_add(name, Element::try_new(ty, i, g).unwrap(), true, true)
                    .unwrap();
            }
            mesh.groups_mut()
                .element_group_mut(name)
                .unwrap()
                .optimize();
        }
        let dat = "contact_detector [\n  type = explicit\n  master = master\n  slave = slave\n  \
                   projection_tolerance = 1e-10\n  max_iterations = 100\n  extension_tolerance = 1e-5\n]\n";
        let parser = Parser::parse_str(dat, "tie.dat").unwrap();
        let selector = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
        let mut positions = Array::<Real>::new(2);
        for p in nodes {
            positions.push_tuple(p);
        }
        let mut detector = ContactDetector::with_positions(2, &positions, &parser, "tie").unwrap();
        detector.set_surface_selector(Box::new(selector));
        let view = MeshView::new(&mesh);
        detector.node_to_elements(&view).unwrap();
        detector.compute_maximal_detection_distance(&view).unwrap();
        let (intersection, grid) = detector.global_search(&view).unwrap();
        detector.local_search(&view, &intersection, &grid).unwrap();
        let origin = grid.cell_id(&nodes[0]).unwrap();
        let cells = nodes
            .iter()
            .map(|p| {
                let c = grid.cell_id(p).unwrap();
                [c.ids()[0] - origin.ids()[0], c.ids()[1] - origin.ids()[1]]
            })
            .collect();
        (detector.contact_pairs().to_vec(), cells)
    }

    /// CONT-008: the local search takes the
    /// **last** of equidistant master nodes in upstream's visiting order,
    /// the neighbouring cells first coordinate fastest
    /// (`contact_detector.cc:165-181`, `neighbor_cells_iterator`). The
    /// slave node 0 at the origin has two masters at distance 1.25 exactly
    /// (dyadic coordinates): A in the cell (+1, −1) and B in the cell
    /// (−1, 0) relative to its own. First coordinate fastest visits A
    /// (row −1) before B (row 0), so B wins; the other order (second
    /// coordinate fastest) would visit the column −1 first and pick A.
    /// Within one cell the items are visited in insertion order (the
    /// sorted master list), so of two equidistant masters in the same cell
    /// the larger node wins.
    #[test]
    fn local_search_tie_follows_upstream_cell_and_insertion_order() {
        // nodes: 0 S, 1 S' (slaves); 2 A, 3 A', 4 B, 5 B' (masters)
        let nodes = [
            [0.0, 0.0],
            [-3.0, -0.5],
            [0.75, -1.0],
            [1.75, -1.0],
            [-1.25, 0.0],
            [-2.25, 0.0],
        ];
        let (pairs, cells) = pairs_and_cells(&nodes, &[[2, 3], [4, 5]], &[[0, 1]]);
        // preconditions of the test geometry (grid spacing √2, centre
        // (−1.125, −0.25))
        assert_eq!(cells[2], [1, -1]);
        assert_eq!(cells[4], [-1, 0]);
        let s = pairs.iter().find(|p| p.0 == 0).copied();
        assert_eq!(s, Some((0, 4)), "pairs {pairs:?}");
        // Upstream's `ContactDetector` on this geometry
        // (pinned x86_64 oracle, three identical runs of a
        // probe, case `tie_cells`): the pairs (slave, master)
        // and the cell of every node (slave node 0 at the origin cell)
        assert_eq!(pairs, vec![(0, 4), (1, 5)]);
        assert_eq!(
            cells,
            vec![[0, 0], [-2, -1], [1, -1], [2, -1], [-1, 0], [-1, 0]]
        );

        // same cell: masters 2 = (−0.3, −0.4) and 4 = (0.3, −0.4) at
        // distance 0.5, vertical master segments of length 1; the master
        // segment 6–7 far left moves the grid centre to (−1, −0.45) so that
        // 2 and 4 share a cell
        let nodes = [
            [0.0, 0.0],
            [-3.0, -0.5],
            [-0.3, -0.4],
            [-0.3, -1.4],
            [0.3, -0.4],
            [0.3, -1.4],
            [-2.0, -0.4],
            [-2.0, -1.4],
        ];
        let (pairs, cells) = pairs_and_cells(&nodes, &[[2, 3], [4, 5], [6, 7]], &[[0, 1]]);
        assert_eq!(cells[2], cells[4]);
        let s = pairs.iter().find(|p| p.0 == 0).copied();
        assert_eq!(s, Some((0, 4)), "pairs {pairs:?}");
        // Upstream's pairs and node cells on this
        // geometry (same artifact, case `tie_same_cell`)
        assert_eq!(pairs, vec![(0, 4), (1, 6)]);
        assert_eq!(
            cells,
            vec![
                [0, 0],
                [-2, -1],
                [0, 0],
                [0, -1],
                [0, 0],
                [0, -1],
                [-1, 0],
                [-1, -1]
            ]
        );
    }

    /// [`GridSpacing::ContactSurface`]: a long side facet at a master node
    /// sets upstream's cell size (√2 · its in-radius) but not the
    /// contact-surface one; on this geometry both find the same pairs.
    #[test]
    fn grid_spacing_from_the_contact_surface() {
        let ty = ElementType::Segment2;
        let g = GhostType::NotGhost;
        // masters: 0..=8 along y = 0 (segments of 0.5); side: 8 – 9 down to
        // y = −100; slaves: 10..=17 along y = 0.05
        let mut nodes: Vec<[Real; 2]> = (0..=8).map(|i| [0.5 * Real::from(i), 0.0]).collect();
        nodes.push([4.0, -100.0]);
        nodes.extend((0..8).map(|i| [0.3 + 0.5 * Real::from(i), 0.05]));
        let master: Vec<[Idx; 2]> = (0..8).map(|i| [i, i + 1]).collect();
        let side: Vec<[Idx; 2]> = vec![[8, 9]];
        let slave: Vec<[Idx; 2]> = (10..17).map(|i| [i + 1, i]).collect();
        let mut mesh = Mesh::new(2, "spacing").unwrap();
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for p in &nodes {
                a.push_node(p).unwrap();
            }
            for c in master.iter().chain(&side).chain(&slave) {
                a.connectivity_mut(ty, g).push_tuple(c);
            }
        }
        let n_master = master.len();
        for (name, first, n) in [
            ("master", 0, n_master),
            ("side", n_master, 1),
            ("slave", n_master + 1, slave.len()),
        ] {
            mesh.groups_mut().create_element_group(name, 1).unwrap();
            for i in first..first + n {
                mesh.element_group_add(name, Element::try_new(ty, i, g).unwrap(), true, true)
                    .unwrap();
            }
            mesh.groups_mut()
                .element_group_mut(name)
                .unwrap()
                .optimize();
        }
        let dat = "contact_detector [\n  type = explicit\n  master = master\n  slave = slave\n  \
                   projection_tolerance = 1e-10\n  max_iterations = 100\n  extension_tolerance = 1e-5\n]\n";
        let parser = Parser::parse_str(dat, "spacing.dat").unwrap();
        let mut positions = Array::<Real>::new(2);
        for p in &nodes {
            positions.push_tuple(p);
        }
        let view = MeshView::new(&mesh);
        let mut result = Vec::new();
        for spacing in [GridSpacing::Upstream, GridSpacing::ContactSurface] {
            let selector = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
            let mut detector =
                ContactDetector::with_positions(2, &positions, &parser, "spacing").unwrap();
            detector.set_surface_selector(Box::new(selector));
            detector.grid_spacing = spacing;
            detector.node_to_elements(&view).unwrap();
            detector.compute_maximal_detection_distance(&view).unwrap();
            let (intersection, grid) = detector.global_search(&view).unwrap();
            detector.local_search(&view, &intersection, &grid).unwrap();
            result.push((
                grid.spacing()[0],
                grid.nb_cells(),
                detector.contact_pairs().to_vec(),
            ));
        }
        // upstream: the side facet's in-radius (half its length for a
        // segment) sets the cells, all masters share one or two cells
        assert!(result[0].0 > 50.0, "{result:?}");
        assert!(result[1].0 < 1.0, "{result:?}");
        assert!(result[1].1 > result[0].1, "{result:?}");
        assert_eq!(result[0].2, result[1].2);
        assert_eq!(result[0].2.len(), 8);
    }
}
