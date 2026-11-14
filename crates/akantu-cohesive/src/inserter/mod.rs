// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh_utils/cohesive_element_inserter.{hh,cc},
// src/mesh_utils/cohesive_element_inserter_inline_impl.hh.

//! [`CohesiveElementInserter`] (MUTIL-015/016, COH-003/005): which facets
//! may receive a cohesive element (`check_facets`: limits, physical
//! surfaces and zones), which ones will (`insertion_facets`), and the
//! insertion itself inside a [`MeshTransaction`] (facet and node doubling,
//! `helper::InserterHelper`).

pub(crate) mod helper;
pub mod parallel;

use std::collections::BTreeSet;

use akantu_core::tensor::MatrixX;
use akantu_core::{
    Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
    TypeFilter,
};
use akantu_mesh::utils::{
    ELEMENT_TO_SUBELEMENT, MATH_TOLERANCE, MESH_TO_MESH_FACET, PHYSICAL_NAMES,
};
use akantu_mesh::{Mesh, MeshEvent, MeshPart, MeshTransaction};
use akantu_parser::{ParserSection, SearchScope};

/// Origin of the parent's events (upstream `insertElements`'
/// `AKANTU_CURRENT_FUNCTION`).
pub const INSERT_ELEMENTS_ORIGIN: &str = "CohesiveElementInserter::insertElements";

/// Topology result of one insertion pass.
#[derive(Clone, Debug, Default)]
pub struct InsertionResult {
    /// New cohesive elements (parent mesh), in creation order.
    pub new_elements: Vec<Element>,
    /// `(old node, new node)` pairs, in creation order.
    pub doubled_nodes: Vec<(Idx, Idx)>,
    /// `(old facet, new facet)` pairs of the doubled facets.
    pub doubled_facets: Vec<(Element, Element)>,
}

/// Upstream `CohesiveElementInserter`.
#[derive(Clone, Debug)]
pub struct CohesiveElementInserter {
    id: String,
    is_extrinsic: bool,
    /// `cohesive_surfaces`: facet groups (physical names) allowed.
    pub physical_surfaces: BTreeSet<String>,
    /// `cohesive_zones`: facets whose two neighbours belong to these groups.
    pub physical_zones: BTreeSet<String>,
    /// `bounding_box`: `(min, max)` per spatial direction.
    insertion_limits: Vec<(Real, Real)>,
    insertion_facets: ElementTypeMapArray<bool>,
    check_facets: ElementTypeMapArray<bool>,
    /// Upstream `Math::getTolerance()` (bounding-box test).
    pub tolerance: Real,
    /// **UD-808** compat (set from
    /// [`crate::CohesiveUpstreamCompat::zones_unnamed_neighbour_error`] by
    /// the model before each zone check): with `cohesive_zones`, a facet
    /// neighbour whose element type has no `physical_names` array (a
    /// cohesive element) is upstream's exception instead of "outside every
    /// zone".
    pub upstream_zone_name_error: bool,
}

/// Upstream `debug::demangle(typeid(std::string).name())` (libstdc++, C++11
/// ABI) in the message of a missing `ElementTypeMapArray<std::string>` type.
const UPSTREAM_STRING_TYPE: &str =
    "std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >";

/// Upstream's exception for a type missing from the non-const
/// `ElementTypeMapArray<std::string>` `mesh:mesh_data:physical_names`
/// (`element_type_map_tmpl.hh:299-307`, `printType`: `(ghost:type)`).
fn upstream_missing_name_type(mesh: &Mesh, el: Element) -> Error {
    Error::Mesh(format!(
        "No element of type ({}:{}) in this ElementTypeMapArray<{UPSTREAM_STRING_TYPE}> class (\"{}:mesh_data:{PHYSICAL_NAMES}\")",
        el.ghost_type(),
        el.ty(),
        mesh.id()
    ))
}

/// Upstream `ElementTypeMapArray::initialize(mesh_facets, _spatial_dimension
/// = dim − 1, _with_nb_element = true, _default_value = v)`: one entry per
/// facet of every facet type (both ghost types, all kinds), existing values
/// kept, new entries `v`.
fn fit_facet_flags(map: &mut ElementTypeMapArray<bool>, fm: &Mesh, value: bool) -> Result<()> {
    let sub = fm.spatial_dimension() - 1;
    for ghost in GhostType::ALL {
        for ty in fm.element_types(TypeFilter {
            spatial_dimension: Some(sub),
            ghost_type: ghost,
            kind: None,
        }) {
            let n = fm.nb_element(ty, ghost);
            if !map.exists(ty, ghost) {
                map.alloc(n, 1, ty, ghost, value)?;
            } else {
                map.get_mut(ty, ghost)?.try_resize(n, value)?;
            }
        }
    }
    Ok(())
}

fn facet_mesh(mesh: &Mesh) -> Result<&Mesh> {
    mesh.mesh_facets()
        .ok_or_else(|| Error::Mesh("the cohesive inserter needs the facet mesh".into()))
}

impl CohesiveElementInserter {
    /// Upstream constructor: limits `±DBL_MAX`, `insertion_facets` sized to
    /// the facets of `mesh`'s facet mesh (which must exist, upstream
    /// `mesh.initMeshFacets()`), all `false`.
    ///
    /// # Errors
    /// A mesh without facet mesh.
    pub fn new(mesh: &Mesh, id: impl Into<String>) -> Result<Self> {
        let fm = facet_mesh(mesh)?;
        let dim = mesh.spatial_dimension();
        let mut insertion_facets = ElementTypeMapArray::new("insertion_facets");
        fit_facet_flags(&mut insertion_facets, fm, false)?;
        Ok(Self {
            id: id.into(),
            is_extrinsic: false,
            physical_surfaces: BTreeSet::new(),
            physical_zones: BTreeSet::new(),
            insertion_limits: vec![(-Real::MAX, Real::MAX); dim],
            insertion_facets,
            check_facets: ElementTypeMapArray::new("check_facets"),
            tolerance: MATH_TOLERANCE,
            upstream_zone_name_error: false,
        })
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Upstream `setIsExtrinsic`.
    pub fn set_is_extrinsic(&mut self, extrinsic: bool) {
        self.is_extrinsic = extrinsic;
    }

    /// Upstream `getIsExtrinsic`.
    #[must_use]
    pub fn is_extrinsic(&self) -> bool {
        self.is_extrinsic
    }

    /// Upstream `parseSection`: `cohesive_surfaces`, `cohesive_zones`,
    /// `bounding_box` (others are errors unless `permissive`), then, when
    /// extrinsic, `limitCheckFacets()`.
    ///
    /// # Errors
    /// Conversion errors, unknown parameters (not permissive).
    pub fn parse_section(
        &mut self,
        mesh: &Mesh,
        section: &ParserSection,
        permissive: bool,
    ) -> Result<()> {
        for p in section.parameters_sorted() {
            match p.name.as_bytes() {
                b"cohesive_surfaces" => {
                    self.physical_surfaces = section.get_parameter_value::<BTreeSet<String>>(
                        "cohesive_surfaces",
                        SearchScope::Current,
                    )?;
                }
                b"cohesive_zones" => {
                    self.physical_zones = section.get_parameter_value::<BTreeSet<String>>(
                        "cohesive_zones",
                        SearchScope::Current,
                    )?;
                }
                b"bounding_box" => {
                    let m: MatrixX =
                        section.get_parameter_value("bounding_box", SearchScope::Current)?;
                    let d = self.insertion_limits.len();
                    for (k, lim) in self.insertion_limits.iter_mut().enumerate().take(d) {
                        if k < m.rows() && m.cols() >= 2 {
                            *lim = (m.at(k, 0), m.at(k, 1));
                        }
                    }
                }
                other => {
                    if !permissive {
                        return Err(Error::invalid(format!(
                            "parameter '{}' is not registered in {}",
                            String::from_utf8_lossy(other),
                            self.id
                        )));
                    }
                }
            }
        }
        if self.is_extrinsic {
            self.limit_check_facets(mesh)?;
        }
        Ok(())
    }

    /// Upstream `setLimit(axis, a, b)` (sorted).
    ///
    /// # Errors
    /// An axis beyond the spatial dimension.
    pub fn set_limit(&mut self, axis: usize, first: Real, second: Real) -> Result<()> {
        let lim = self
            .insertion_limits
            .get_mut(axis)
            .ok_or(Error::OutOfRange {
                what: "insertion limit axis",
                index: axis,
                len: 0,
            })?;
        *lim = (first.min(second), first.max(second));
        Ok(())
    }

    /// Upstream `addPhysicalSurface`.
    pub fn add_physical_surface(&mut self, name: impl Into<String>) {
        self.physical_surfaces.insert(name.into());
    }

    /// Upstream `addPhysicalVolume` (`cohesive_zones`).
    pub fn add_physical_volume(&mut self, name: impl Into<String>) {
        self.physical_zones.insert(name.into());
    }

    /// Facets that will receive a cohesive element at the next insertion
    /// (upstream `getInsertionFacetsByElement`).
    #[must_use]
    pub fn insertion_facets(&self) -> &ElementTypeMapArray<bool> {
        &self.insertion_facets
    }

    /// Mutable [`CohesiveElementInserter::insertion_facets`] (materials'
    /// `checkInsertion`, user code).
    pub fn insertion_facets_mut(&mut self) -> &mut ElementTypeMapArray<bool> {
        &mut self.insertion_facets
    }

    /// `(check_facets, insertion_facets)` at once (the laws' checks read
    /// the first and flag the second).
    pub fn flags_mut(&mut self) -> (&ElementTypeMapArray<bool>, &mut ElementTypeMapArray<bool>) {
        (&self.check_facets, &mut self.insertion_facets)
    }

    /// Facets where a stress check is performed (upstream `getCheckFacets`).
    #[must_use]
    pub fn check_facets(&self) -> &ElementTypeMapArray<bool> {
        &self.check_facets
    }

    /// Mutable [`CohesiveElementInserter::check_facets`] (upstream's
    /// non-const `getCheckFacets(type, ghost_type)`; e.g.
    /// `test_cohesive_parallel_buildfragments`' `verticalInsertionLimit`
    /// keeps only the facets of chosen planes). Changing the flags after
    /// `init_full` changes which facets the next
    /// [`crate::SolidMechanicsModelCohesive::check_cohesive_stress`] checks;
    /// on a distributed mesh the facet-stress synchronizer keeps the
    /// facets it had (a superset, as upstream) until the next insertion
    /// prunes it.
    pub fn check_facets_mut(&mut self) -> &mut ElementTypeMapArray<bool> {
        &mut self.check_facets
    }

    /// Upstream `limitCheckFacets()` on `check_facets`.
    ///
    /// # Errors
    /// Malformed facet adjacency or mesh data.
    pub fn limit_check_facets(&mut self, mesh: &Mesh) -> Result<()> {
        let mut check = std::mem::take(&mut self.check_facets);
        let r = self.limit_check_facets_into(mesh, &mut check);
        self.check_facets = check;
        r
    }

    /// Upstream `limitCheckFacets(check_facets)`: every facet `true`, then
    /// `false` for boundary facets (second neighbour null), facets between
    /// two ghosts, facets next to a cohesive element, facets whose
    /// barycenter is outside the `bounding_box` (± `Math::tolerance`),
    /// facets whose two neighbours are not both in `cohesive_zones` (if
    /// given) and facets outside `cohesive_surfaces` (if given).
    fn limit_check_facets_into(
        &self,
        mesh: &Mesh,
        check: &mut ElementTypeMapArray<bool>,
    ) -> Result<()> {
        let fm = facet_mesh(mesh)?;
        let sd = mesh.spatial_dimension();
        fit_facet_flags(check, fm, true)?;
        check.fill(true);
        let e2s = fm.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
        let facets: Vec<Element> = GhostType::ALL
            .into_iter()
            .flat_map(|g| {
                fm.elements(TypeFilter {
                    spatial_dimension: Some(sd - 1),
                    ghost_type: g,
                    kind: Some(ElementKind::Regular),
                })
            })
            .collect();
        for &facet in &facets {
            let row = &e2s.element(facet)?[0];
            let left = row.first().copied().unwrap_or(Element::NULL);
            let right = row.get(1).copied().unwrap_or(Element::NULL);
            let slot = &mut check.element_mut(facet)?[0];
            if right.is_null()
                || (left.ghost_type() == GhostType::Ghost && right.ghost_type() == GhostType::Ghost)
            {
                *slot = false;
                continue;
            }
            if left.kind() == ElementKind::Cohesive || right.kind() == ElementKind::Cohesive {
                *slot = false;
            }
        }
        let mut bary = vec![0.0; sd];
        for &facet in &facets {
            if !check.element(facet)?[0] {
                continue;
            }
            fm.barycenter_into(facet, &mut bary)?;
            let mut k = 0;
            while k < sd
                && bary[k] > self.insertion_limits[k].0 - self.tolerance
                && bary[k] < self.insertion_limits[k].1 + self.tolerance
            {
                k += 1;
            }
            if k != sd {
                check.element_mut(facet)?[0] = false;
            }
        }
        if mesh.data().has_elemental(PHYSICAL_NAMES) && !self.physical_zones.is_empty() {
            let names = mesh.data().strings(PHYSICAL_NAMES)?;
            for &facet in &facets {
                let row = &e2s.element(facet)?[0];
                let mut count = 0;
                for el in row.iter().take(2) {
                    if el.is_null() {
                        continue;
                    }
                    // upstream reads `physical_names(element)` for any
                    // neighbour and throws for a type without names (a
                    // cohesive neighbour, once an element is inserted):
                    // UD-808 (B) — by default such a neighbour is outside
                    // every zone (the facet is already excluded by the
                    // cohesive-neighbour rule above); the compat switch
                    // reproduces upstream's exception
                    if self.upstream_zone_name_error && !names.exists(el.ty(), el.ghost_type()) {
                        return Err(upstream_missing_name_type(mesh, *el));
                    }
                    if let Ok(n) = names.element(*el)
                        && n.first().is_some_and(|n| self.physical_zones.contains(n))
                    {
                        count += 1;
                    }
                }
                if count != 2 {
                    check.element_mut(facet)?[0] = false;
                }
            }
        }
        if self.physical_surfaces.is_empty() {
            return Ok(());
        }
        if !fm.data().has_elemental(PHYSICAL_NAMES) {
            // upstream: debug assertion only
            return Ok(());
        }
        let ids = fm.data().strings(PHYSICAL_NAMES)?;
        for &facet in &facets {
            if !check.element(facet)?[0] {
                continue;
            }
            let inside = ids
                .element(facet)
                .ok()
                .and_then(|n| n.first())
                .is_some_and(|n| self.physical_surfaces.contains(n));
            check.element_mut(facet)?[0] = inside;
        }
        Ok(())
    }

    /// Upstream `insertIntrinsicElements()` topology part: every allowed
    /// facet is flagged for insertion (`limitCheckFacets(insertion_facets)`).
    ///
    /// # Errors
    /// See [`CohesiveElementInserter::limit_check_facets`].
    pub fn flag_intrinsic_facets(&mut self, mesh: &Mesh) -> Result<()> {
        let mut ins = std::mem::take(&mut self.insertion_facets);
        let r = self.limit_check_facets_into(mesh, &mut ins);
        self.insertion_facets = ins;
        r
    }

    /// Topology stage of upstream `insertElements(only_double_facets)`
    /// inside a [`MeshTransaction`]: double the flagged facets and their
    /// nodes, create the cohesive elements (unless `only_double_facets`),
    /// record the facet events and then the parent's `ElementsAdded` (new
    /// cohesive elements) and `NodesAdded` (with `old_nodes`) events. The
    /// inserter's own flags are updated afterwards by
    /// [`CohesiveElementInserter::update_insertion_facets`] (only after the
    /// mutation committed).
    ///
    /// # Errors
    /// Malformed adjacency, a mesh distributed over several ranks (use
    /// [`CohesiveElementInserter::insert_elements_ordered_in`] with the
    /// global insertion order), transaction errors.
    pub fn insert_elements_in(
        &self,
        tx: &mut MeshTransaction<'_>,
        only_double_facets: bool,
    ) -> Result<InsertionResult> {
        self.insert_elements_ordered_in(tx, only_double_facets, None)
    }

    /// [`CohesiveElementInserter::insert_elements_in`] with the facets to
    /// double in `order` (the global insertion order of
    /// [`CohesiveElementInserter::synchronize_insertion_in`], required on a
    /// mesh distributed over several ranks; `None`: the flagged facets in
    /// serial order).
    ///
    /// # Errors
    /// See [`CohesiveElementInserter::insert_elements_in`].
    pub fn insert_elements_ordered_in(
        &self,
        tx: &mut MeshTransaction<'_>,
        only_double_facets: bool,
        order: Option<&[Element]>,
    ) -> Result<InsertionResult> {
        let mut helper = helper::InserterHelper::new(tx, &self.insertion_facets, order)?;
        let nb_new = if only_double_facets {
            helper.insert_facets_only(tx)?;
            0
        } else {
            helper.insert_cohesive_elements(tx)?
        };
        let result = InsertionResult {
            new_elements: helper.new_elements.clone(),
            doubled_nodes: helper.doubled_nodes.clone(),
            doubled_facets: helper.doubled_facets().to_vec(),
        };
        debug_assert_eq!(nb_new, result.new_elements.len());
        // distributed mesh: the same event kinds on every rank (empty lists
        // on a rank without facet to double)
        let record_empty = order.is_some();
        if !result.new_elements.is_empty() || record_empty {
            tx.record(
                MeshPart::Main,
                MeshEvent::ElementsAdded {
                    elements: result.new_elements.clone(),
                    constitutive_law: None,
                    origin: INSERT_ELEMENTS_ORIGIN.into(),
                    extension: None,
                },
            );
        }
        if !result.doubled_nodes.is_empty() || record_empty {
            // upstream rewrites the boundary connectivities in the node
            // event's Mesh::sendEvent: on a serial mesh when this insertion
            // doubled a node; on a distributed mesh the cohesive hook does it
            // when updateGlobalIDs(…) > 0 (UD-1250)
            if !result.doubled_nodes.is_empty() && !tx.mesh().is_distributed() {
                sync_boundary_connectivity(tx)?;
            }
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes: result.doubled_nodes.iter().map(|p| p.1).collect(),
                    old_nodes: Some(result.doubled_nodes.iter().map(|p| p.0).collect()),
                    origin: INSERT_ELEMENTS_ORIGIN.into(),
                    extension: None,
                },
            );
        }
        let _ = helper.nb_facets_to_double();
        Ok(result)
    }

    /// Upstream `updateInsertionFacets()` (after a pass that inserted
    /// elements): extrinsic — inserted facets are no longer flagged nor
    /// checked; both maps grow to the new facet count (new facets `false`);
    /// intrinsic — every flag is cleared.
    ///
    /// # Errors
    /// A mesh without facet mesh.
    pub fn update_insertion_facets(&mut self, mesh: &Mesh) -> Result<()> {
        let fm = facet_mesh(mesh)?;
        if self.is_extrinsic {
            for ghost in GhostType::ALL {
                let types: Vec<ElementType> = self
                    .insertion_facets
                    .element_types(TypeFilter {
                        spatial_dimension: Some(mesh.spatial_dimension() - 1),
                        ghost_type: ghost,
                        kind: None,
                    })
                    .collect();
                for ty in types {
                    let Ok(check) = self.check_facets.get_mut(ty, ghost) else {
                        continue;
                    };
                    let ins = self.insertion_facets.get_mut(ty, ghost)?;
                    for (i, c) in ins.as_mut_slice().iter_mut().zip(check.as_mut_slice()) {
                        if *i {
                            *i = false;
                            *c = false;
                        }
                    }
                }
            }
        }
        fit_facet_flags(&mut self.insertion_facets, fm, false)?;
        if self.is_extrinsic {
            fit_facet_flags(&mut self.check_facets, fm, false)?;
        } else {
            self.insertion_facets.fill(false);
        }
        Ok(())
    }
}

/// Upstream `Mesh::sendEvent<NewNodesEvent>` for a `CohesiveNewNodesEvent`
/// (`mesh.cc:118-140`): every element of dimension `dim − 1` of the parent
/// with a `mesh_to_mesh_facet` entry takes the connectivity of its facet
/// (doubled nodes propagate to boundary/physical-group elements). A
/// mesh-owned dependent (journaled; only changed rows are written): applied
/// in the topology stage on a serial mesh that doubled a node, and by the
/// distributed cohesive hook when upstream would send its node event
/// (`updateGlobalIDs(…) > 0`, UD-1250).
pub(crate) fn sync_boundary_connectivity(tx: &mut MeshTransaction<'_>) -> Result<()> {
    let mut updates: Vec<(Element, Vec<Idx>)> = Vec::new();
    {
        let m = tx.mesh();
        let Some(fm) = m.mesh_facets() else {
            return Ok(());
        };
        let Ok(m2f) = m.data().element_map(MESH_TO_MESH_FACET) else {
            return Ok(());
        };
        for ghost in GhostType::ALL {
            for ty in m.element_types(TypeFilter {
                spatial_dimension: Some(m.spatial_dimension() - 1),
                ghost_type: ghost,
                kind: Some(ElementKind::Regular),
            }) {
                let Ok(map) = m2f.get(ty, ghost) else {
                    continue;
                };
                let conn = m.connectivity(ty, ghost)?;
                let nn = conn.nb_component();
                for (e, (row, facet)) in conn
                    .as_slice()
                    .chunks_exact(nn)
                    .zip(map.as_slice())
                    .enumerate()
                {
                    if facet.is_null() {
                        continue;
                    }
                    let fc = fm.element_connectivity(*facet)?;
                    if fc != row {
                        updates.push((Element::try_new(ty, e, ghost)?, fc.to_vec()));
                    }
                }
            }
        }
    }
    for (el, conn) in updates {
        tx.set_element_nodes(MeshPart::Main, el, &conn)?;
    }
    Ok(())
}
