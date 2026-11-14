// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_cohesive/fragment_manager.{hh,cc}.

//! [`FragmentManager`] (COH-012): fragments = clusters of bulk elements
//! connected through facets or through cohesive elements that are not
//! fully broken; their masses, centres of mass, velocities, inertia
//! moments (principal values and directions) and element counts.
//!
//! **Distributed meshes** (COH-008; upstream
//! `fragment_manager.cc:112-143`, `:350-433`, `:436-462`): `buildFragments`
//! first gives the ghost cohesive elements their owners' damage
//! (`_smmc_damage` over the cohesive synchronizer), then clusters through
//! the distributed `createClusters` (global clusters, numbered over every
//! rank: `akantu_mesh::clusters::compute_global_clusters_in`; every rank
//! holds every fragment, empty where it has none of its elements, the
//! elements of a fragment sorted per type as upstream's `optimize` leaves
//! them). Only the rank's **not-ghost** bulk elements contribute to the
//! integrals and the element counts; each integrated array (masses,
//! `∫ ρ x`, `∫ ρ v`, the inertia integrals) and the element counts are
//! then summed over the ranks (upstream `comm.allReduce(_sum)`; the port's
//! reduction is the rank-order fold, D-039). The per-rank contributions of
//! the last computation are kept in [`FragmentManager::local`]. The centres
//! of mass and velocities are divided by the reduced masses, the
//! eigen-decomposition runs on the reduced inertia integrals (identical on
//! every rank).
//!
//! **UD-036** (category B): upstream resets a fragment's accumulator for
//! every element type, so a fragment spanning several types keeps only
//! the last type's integral; the port accumulates over all types by
//! default ([`FragmentManager::upstream_compat_per_type_reset`]). Upstream
//! also writes its dump data into copies (no effect); the port exposes the
//! per-element fragment index ([`FragmentManager::fragment_index_of`])
//! instead of mesh data.

use std::collections::HashSet;

use akantu_comm::phase::{Phases, Severity};
use akantu_comm::{Communicator, ReduceOp};
use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Real, Result,
    TypeFilter,
};
use akantu_linalg::dense::{SortOrder, Storage, eig_vectors};

use crate::material::{COHESIVE_FE_ENGINE, MaterialCohesive};
use crate::model::SolidMechanicsModelCohesive;

/// This rank's contributions (before the sum over the ranks) of the last
/// computation of each quantity (see the module docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FragmentLocalData {
    /// `∫ ρ` per fragment and component (`nb_fragment × dim`).
    pub mass: Vec<Real>,
    /// `∫ ρ x` (`nb_fragment × dim`, before the division by the mass).
    pub mass_center: Vec<Real>,
    /// `∫ ρ v` (`nb_fragment × dim`, before the division by the mass).
    pub velocity: Vec<Real>,
    /// `∫ ρ (tr(r rᵀ) I − r rᵀ)` (`nb_fragment × dim²`, column-major).
    pub inertia: Vec<Real>,
    /// Not-ghost bulk elements per fragment.
    pub nb_elements: Vec<u64>,
}

/// Upstream `FragmentManager`.
#[derive(Clone, Debug)]
pub struct FragmentManager {
    dim: usize,
    /// The mesh's communicator (upstream sums over it).
    comm: Communicator,
    /// Fragments (cluster order: fragment `i` = upstream group
    /// `fragment_<i>`). Serial (and one rank): elements in discovery order
    /// (bulk and cohesive). Distributed over several ranks: every global
    /// fragment, with this rank's elements (local and ghost) sorted by
    /// (ghost type, type, index); empty where the rank holds none.
    pub fragments: Vec<Vec<Element>>,
    /// Per-rank contributions of the last computations.
    pub local: FragmentLocalData,
    /// Masses (`nb_fragment × dim`, every component the mass).
    pub mass: Vec<Real>,
    /// Centres of mass (`nb_fragment × dim`).
    pub mass_center: Vec<Real>,
    /// Mean velocities (`nb_fragment × dim`).
    pub velocity: Vec<Real>,
    /// Principal inertia moments (`nb_fragment × dim`, descending).
    pub inertia_moments: Vec<Real>,
    /// Principal directions (`nb_fragment × dim²`, column-major).
    pub principal_directions: Vec<Real>,
    /// Bulk elements per fragment.
    pub nb_elements_per_fragment: Vec<usize>,
    quad_coordinates: ElementTypeMapArray<Real>,
    mass_densities: ElementTypeMapArray<Real>,
    /// UD-036 compat: reset the accumulator per element type (upstream).
    pub upstream_compat_per_type_reset: bool,
    /// UD-222 compat: upstream `computeInertiaMoments` zips
    /// the fragment groups in **name** order (`fragment_0`, `fragment_1`,
    /// `fragment_10`, …) with the centres of mass in **index** order, so
    /// from 11 fragments on the moments use another fragment's centre.
    /// Default: each fragment's own centre.
    pub upstream_compat_inertia_name_order: bool,
}

fn regular_types(m: &akantu_mesh::Mesh, dim: usize) -> Vec<ElementType> {
    m.element_types(TypeFilter {
        spatial_dimension: Some(dim),
        ghost_type: GhostType::NotGhost,
        kind: Some(ElementKind::Regular),
    })
    .collect()
}

impl FragmentManager {
    /// Upstream constructor: quadrature-point coordinates and mass densities
    /// of the bulk elements.
    ///
    /// # Errors
    /// FE engine or material errors.
    pub fn new(model: &SolidMechanicsModelCohesive) -> Result<Self> {
        let dim = model.smm.spatial_dimension();
        let mesh_h = model.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        let fe = model.smm.physics.base.fems.get("")?;
        let mut quad_coordinates = ElementTypeMapArray::new("quad_coordinates");
        let mut mass_densities = ElementTypeMapArray::new("mass_density");
        for ty in regular_types(&m, dim) {
            let mut out = Array::new(dim);
            fe.interpolate_on_integration_points(
                &m,
                m.nodes().view(),
                &mut out,
                dim,
                ty,
                GhostType::NotGhost,
                None,
            )?;
            quad_coordinates
                .as_map_mut()
                .insert(ty, GhostType::NotGhost, out)?;
            let nq = fe.nb_integration_points(ty, GhostType::NotGhost)?;
            let ne = m.nb_element(ty, GhostType::NotGhost);
            let idx = model
                .smm
                .material_by_element()
                .get(ty, GhostType::NotGhost)?;
            let mut rho = Vec::with_capacity(ne * nq);
            for e in 0..ne {
                let law = akantu_core::decode_idx(idx.as_slice()[e])?
                    .ok_or_else(|| Error::invalid("element without material (FragmentManager)"))?;
                let r = model.smm.material(law)?.rho();
                rho.extend(std::iter::repeat_n(r, nq));
            }
            mass_densities.as_map_mut().insert(
                ty,
                GhostType::NotGhost,
                Array::from_vec(rho, 1)?,
            )?;
        }
        let comm = m.communicator().clone();
        Ok(Self {
            dim,
            comm,
            fragments: Vec::new(),
            local: FragmentLocalData::default(),
            mass: Vec::new(),
            mass_center: Vec::new(),
            velocity: Vec::new(),
            inertia_moments: Vec::new(),
            principal_directions: Vec::new(),
            nb_elements_per_fragment: Vec::new(),
            quad_coordinates,
            mass_densities,
            upstream_compat_per_type_reset: false,
            upstream_compat_inertia_name_order: false,
        })
    }

    /// Number of fragments.
    #[must_use]
    pub fn nb_fragment(&self) -> usize {
        self.fragments.len()
    }

    /// Fragment of `element` (`None` if it belongs to none).
    #[must_use]
    pub fn fragment_index_of(&self, element: Element) -> Option<usize> {
        self.fragments.iter().position(|f| f.contains(&element))
    }

    /// Upstream `buildFragments(damage_limit)`: on a distributed mesh the
    /// ghost cohesive elements take their owners' damage first
    /// (`_smmc_damage`); then clusters through facets and through cohesive
    /// elements (local and ghost) with at least one quadrature point whose
    /// damage is below `damage_limit − 1e-14` (distributed: global
    /// clusters, see the module docs); then the masses.
    ///
    /// # Errors
    /// Mesh, material or FE errors; on a distributed mesh exchange errors
    /// and the refusals of the distributed clustering (agreed).
    pub fn build_fragments(
        &mut self,
        model: &mut SolidMechanicsModelCohesive,
        damage_limit: Real,
    ) -> Result<()> {
        let mesh_h = model.smm.physics.base.mesh.clone();
        if mesh_h.read()?.is_distributed() {
            model.synchronize_cohesive(akantu_core::enums::SynchronizationTag::SmmcDamage)?;
        }
        let m = mesh_h.read()?;
        let fe_c = model.smm.physics.base.fems.get(COHESIVE_FE_ENGINE)?;
        // cohesive elements that still connect (upstream CohesiveElementFilter,
        // evaluated on local and ghost cohesive elements)
        let connecting = (|| -> Result<HashSet<Element>> {
            let mut connecting = HashSet::new();
            for ghost in GhostType::ALL {
                for ty in m.element_types(TypeFilter {
                    spatial_dimension: Some(self.dim),
                    ghost_type: ghost,
                    kind: Some(ElementKind::Cohesive),
                }) {
                    let ne = m.nb_element(ty, ghost);
                    if ne == 0 {
                        continue;
                    }
                    let nq = fe_c.nb_integration_points(ty, ghost)?;
                    let idx = model.smm.material_by_element().get(ty, ghost)?;
                    let loc = model.smm.material_local_numbering().get(ty, ghost)?;
                    for e in 0..ne {
                        let law = akantu_core::decode_idx(idx.as_slice()[e])?
                            .ok_or_else(|| Error::invalid("cohesive element without material"))?;
                        let mat = model
                            .smm
                            .material(law)?
                            .as_any()
                            .and_then(|a| a.downcast_ref::<MaterialCohesive>())
                            .ok_or_else(|| {
                                Error::invalid("cohesive element with a non-cohesive material")
                            })?;
                        let l = akantu_core::try_to_usize(loc.as_slice()[e])?;
                        let d_g = mat.array("damage", ty, ghost)?;
                        let d = d_g.as_slice();
                        let quads = d.get(l * nq..(l + 1) * nq).ok_or(Error::OutOfRange {
                            what: "cohesive damage quadrature point",
                            index: l * nq,
                            len: d.len(),
                        })?;
                        if quads.iter().any(|x| *x < damage_limit - 1e-14) {
                            connecting.insert(Element::try_new(ty, e, ghost)?);
                        }
                    }
                }
            }
            Ok(connecting)
        })();
        let (connecting, local_ok) = match connecting {
            Ok(c) => (c, Ok(())),
            Err(e) => (HashSet::new(), Err(e)),
        };
        let filter = |el: Element| el.kind() == ElementKind::Regular || connecting.contains(&el);
        let mut ph = Phases::new(m.communicator(), "fragment_manager:build_fragments");
        let r = (|| -> Result<Vec<Vec<Element>>> {
            // a local failure (facet mesh, damage arrays) is agreed before the
            // collective clustering
            let fm = local_ok.and_then(|()| {
                m.mesh_facets()
                    .ok_or_else(|| Error::Mesh("no facet mesh".into()))
            });
            let fm = ph.agreed("fragments:filter", fm, Severity::Recoverable)?;
            akantu_mesh::clusters::compute_global_clusters_in(&mut ph, &m, fm, self.dim, &filter)
        })();
        let fragments = ph.settle(r);
        drop(ph);
        self.fragments = fragments?;
        drop(m);
        self.compute_mass(model)
    }

    /// Upstream `comm.allReduce(output, _sum)` (rank-order fold, D-039).
    fn sum_over_ranks<T: akantu_comm::CommData>(&self, what: &str, values: &mut [T]) -> Result<()> {
        let mut ph = Phases::new(&self.comm, format!("fragment_manager:{what}"));
        let r = ph.collective(what, |c| c.all_reduce(values, ReduceOp::Sum));
        ph.settle(r)
    }

    /// Upstream `integrateFieldOnFragments(field, output)` before its sum
    /// over the ranks: `∫ ρ f` over this rank's not-ghost bulk elements of
    /// every fragment.
    fn integrate_on_fragments(
        &self,
        model: &SolidMechanicsModelCohesive,
        field: &ElementTypeMapArray<Real>,
        nb_component: usize,
    ) -> Result<Vec<Real>> {
        let fe = model.smm.physics.base.fems.get("")?;
        let mut out = vec![0.0; self.fragments.len() * nb_component];
        for (fi, frag) in self.fragments.iter().enumerate() {
            let acc = &mut out[fi * nb_component..(fi + 1) * nb_component];
            // this rank's not-ghost bulk elements (upstream
            // `elementTypes(dim, _not_ghost, _ek_regular)` of the group)
            let mut types: Vec<ElementType> = frag
                .iter()
                .filter(|e| {
                    e.kind() == ElementKind::Regular && e.ghost_type() == GhostType::NotGhost
                })
                .map(|e| e.ty())
                .collect();
            types.sort_by_key(|t| t.index());
            types.dedup();
            for ty in types {
                let elems: Vec<akantu_core::Idx> = frag
                    .iter()
                    .filter(|e| e.ty() == ty && e.ghost_type() == GhostType::NotGhost)
                    .map(|e| e.index())
                    .collect();
                let nq = fe.nb_integration_points(ty, GhostType::NotGhost)?;
                let f = field.get(ty, GhostType::NotGhost)?.as_slice();
                let rho = self.mass_densities.get(ty, GhostType::NotGhost)?.as_slice();
                let mut ia = Vec::with_capacity(elems.len() * nq * nb_component);
                for &ge in &elems {
                    let ge = akantu_core::try_to_usize(ge)?;
                    for q in 0..nq {
                        for j in 0..nb_component {
                            let mut v = f[(ge * nq + q) * nb_component + j];
                            v *= rho[ge * nq + q];
                            ia.push(v);
                        }
                    }
                }
                let mut integrated = Array::new(nb_component);
                fe.integrate(
                    akantu_core::array::ArrayView::new(&ia, nb_component)?,
                    &mut integrated,
                    nb_component,
                    ty,
                    GhostType::NotGhost,
                    Some(&elems),
                )?;
                if self.upstream_compat_per_type_reset {
                    acc.fill(0.0);
                }
                for row in integrated.as_slice().chunks_exact(nb_component) {
                    for (a, r) in acc.iter_mut().zip(row) {
                        *a += r;
                    }
                }
            }
        }
        Ok(out)
    }

    fn unit_field(
        &self,
        model: &SolidMechanicsModelCohesive,
        nbc: usize,
        value: Real,
    ) -> Result<ElementTypeMapArray<Real>> {
        let mesh_h = model.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        let fe = model.smm.physics.base.fems.get("")?;
        let mut f = ElementTypeMapArray::new("unit_field");
        for ty in regular_types(&m, self.dim) {
            let n = m.nb_element(ty, GhostType::NotGhost)
                * fe.nb_integration_points(ty, GhostType::NotGhost)?;
            f.alloc(n, nbc, ty, GhostType::NotGhost, value)?;
        }
        Ok(f)
    }

    /// Upstream `computeMass()`.
    ///
    /// # Errors
    /// FE errors.
    pub fn compute_mass(&mut self, model: &SolidMechanicsModelCohesive) -> Result<()> {
        let unit = self.unit_field(model, self.dim, 1.)?;
        let local = self.integrate_on_fragments(model, &unit, self.dim)?;
        let mut mass = local.clone();
        self.sum_over_ranks("mass", &mut mass)?;
        self.local.mass = local;
        self.mass = mass;
        Ok(())
    }

    /// Upstream `computeCenterOfMass()`.
    ///
    /// # Errors
    /// FE errors.
    pub fn compute_center_of_mass(&mut self, model: &SolidMechanicsModelCohesive) -> Result<()> {
        let local = self.integrate_on_fragments(model, &self.quad_coordinates, self.dim)?;
        let mut c = local.clone();
        self.sum_over_ranks("mass_center", &mut c)?;
        self.local.mass_center = local;
        for (x, m) in c.iter_mut().zip(&self.mass) {
            *x /= m;
        }
        self.mass_center = c;
        Ok(())
    }

    /// Upstream `computeVelocity()`.
    ///
    /// # Errors
    /// FE errors.
    pub fn compute_velocity(&mut self, model: &SolidMechanicsModelCohesive) -> Result<()> {
        let mesh_h = model.smm.physics.base.mesh.clone();
        let mut vf = ElementTypeMapArray::new("velocity_field");
        {
            let m = mesh_h.read()?;
            let fe = model.smm.physics.base.fems.get("")?;
            let v = model.smm.velocity()?;
            for ty in regular_types(&m, self.dim) {
                let mut out = Array::new(self.dim);
                fe.interpolate_on_integration_points(
                    &m,
                    v.view(),
                    &mut out,
                    self.dim,
                    ty,
                    GhostType::NotGhost,
                    None,
                )?;
                vf.as_map_mut().insert(ty, GhostType::NotGhost, out)?;
            }
        }
        let local = self.integrate_on_fragments(model, &vf, self.dim)?;
        let mut v = local.clone();
        self.sum_over_ranks("velocity", &mut v)?;
        self.local.velocity = local;
        for (x, m) in v.iter_mut().zip(&self.mass) {
            *x /= m;
        }
        self.velocity = v;
        Ok(())
    }

    /// Upstream `computeInertiaMoments()`: `∫ ρ (tr(r rᵀ) I − r rᵀ)` about
    /// the centre of mass, principal values (descending) and directions
    /// (upstream `Matrix::eig`, general Eigen solver on a run-time matrix).
    ///
    /// # Errors
    /// FE or eigen-solver errors.
    pub fn compute_inertia_moments(&mut self, model: &SolidMechanicsModelCohesive) -> Result<()> {
        let d = self.dim;
        self.compute_center_of_mass(model)?;
        let mut moments = self.unit_field(model, d * d, 1.)?;
        let fe = model.smm.physics.base.fems.get("")?;
        // upstream group iteration order: names `fragment_<i>` sorted
        let mut by_name: Vec<usize> = (0..self.fragments.len()).collect();
        by_name.sort_by_key(|i| format!("fragment_{i}"));
        for (pos, &fi) in by_name.iter().enumerate() {
            let frag = &self.fragments[fi];
            let ci = if self.upstream_compat_inertia_name_order {
                pos
            } else {
                fi
            };
            let c = &self.mass_center[ci * d..(ci + 1) * d];
            for el in frag.iter().filter(|e| {
                e.kind() == ElementKind::Regular && e.ghost_type() == GhostType::NotGhost
            }) {
                let nq = fe.nb_integration_points(el.ty(), GhostType::NotGhost)?;
                let qc = self
                    .quad_coordinates
                    .get(el.ty(), GhostType::NotGhost)?
                    .as_slice();
                let mm = moments
                    .get_mut(el.ty(), GhostType::NotGhost)?
                    .as_mut_slice();
                for q in 0..nq {
                    let gq = el.idx() * nq + q;
                    let mut r = [0.0; 3];
                    for i in 0..d {
                        r[i] = qc[gq * d + i];
                        r[i] -= c[i];
                    }
                    let m = &mut mm[gq * d * d..(gq + 1) * d * d];
                    for j in 0..d {
                        for i in 0..d {
                            m[i + d * j] = r[i] * r[j];
                        }
                    }
                    // `trace()` of a run-time matrix (sequential)
                    let mut trace = m[0];
                    for i in 1..d {
                        trace += m[i + d * i];
                    }
                    for j in 0..d {
                        for i in 0..d {
                            let id = if i == j { 1.0 } else { 0.0 };
                            // `-1. * M + trace * I` (multiplication by −1 is exact)
                            m[i + d * j] = -m[i + d * j] + trace * id;
                        }
                    }
                }
            }
        }
        let local = self.integrate_on_fragments(model, &moments, d * d)?;
        let mut integrated = local.clone();
        self.sum_over_ranks("inertia", &mut integrated)?;
        self.local.inertia = local;
        self.inertia_moments = Vec::with_capacity(self.fragments.len() * d);
        self.principal_directions = Vec::with_capacity(self.fragments.len() * d * d);
        for chunk in integrated.chunks_exact(d * d) {
            let a = akantu_core::tensor::MatrixX::from_col_major(d, d, chunk.to_vec())?;
            let (values, vectors) = eig_vectors(&a, Storage::Dynamic, SortOrder::Descending)?;
            self.inertia_moments.extend(values);
            self.principal_directions
                .extend_from_slice(vectors.as_slice());
        }
        Ok(())
    }

    /// Upstream `computeNbElementsPerFragment()`: this rank's not-ghost
    /// bulk elements per fragment, summed over the ranks.
    ///
    /// # Errors
    /// Exchange errors (distributed mesh).
    pub fn compute_nb_elements_per_fragment(&mut self) -> Result<()> {
        let local: Vec<u64> = self
            .fragments
            .iter()
            .map(|f| {
                f.iter()
                    .filter(|e| {
                        e.kind() == ElementKind::Regular && e.ghost_type() == GhostType::NotGhost
                    })
                    .count() as u64
            })
            .collect();
        let mut total = local.clone();
        self.sum_over_ranks("nb_elements", &mut total)?;
        self.local.nb_elements = local;
        self.nb_elements_per_fragment = total
            .into_iter()
            .map(|n| usize::try_from(n).map_err(|_| Error::invalid("fragment element count")))
            .collect::<Result<_>>()?;
        Ok(())
    }

    /// Upstream `computeAllData(damage_limit)`.
    ///
    /// # Errors
    /// See the individual computations.
    pub fn compute_all_data(
        &mut self,
        model: &mut SolidMechanicsModelCohesive,
        damage_limit: Real,
    ) -> Result<()> {
        self.build_fragments(model, damage_limit)?;
        self.compute_velocity(model)?;
        self.compute_inertia_moments(model)?;
        self.compute_nb_elements_per_fragment()
    }
}
