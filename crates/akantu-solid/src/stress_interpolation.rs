// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/material.cc
// (initElementalFieldInterpolation, interpolateStress,
// interpolateStressOnFacets), material.hh (members
// interpolation_inverse_coordinates, interpolation_points_matrices).

//! Stress interpolation of a material at arbitrary points (SMM-024; used by
//! the extrinsic cohesive model to evaluate the bulk stresses on the facet
//! quadrature points).
//!
//! Upstream's `Material` keeps the interpolation matrices as members and
//! reaches the FE engine and the mesh through its model. Here the matrices
//! live in [`MaterialBase::stress_interpolation`] (an
//! [`akantu_fe::engine::ElementalFieldInterpolation`], bound to the material's element ids and
//! to the mesh revision, FE-025) and the FE engine and the mesh are passed
//! in: [`MaterialBase::init_elemental_field_interpolation`],
//! [`MaterialBase::interpolate_stress`] and
//! [`MaterialBase::interpolate_stress_on_facets`] are the three upstream
//! wrappers; [`SolidMechanicsModel::init_material_elemental_field_interpolation`],
//! [`SolidMechanicsModel::interpolate_material_stress`] and
//! [`SolidMechanicsModel::interpolate_material_stress_on_facets`] call them
//! with the model's default FE engine (upstream `Material::getFEEngine()`
//! with the default `fe_engine_id`) and the model's mesh.
//!
//! Deviations, none changing a defined upstream result:
//! - errors instead of upstream's undefined behaviour: `interpolate_stress*`
//!   before `init_elemental_field_interpolation` (upstream:
//!   `ElementTypeMapArray` lookup throws); a facet result block that is
//!   missing or whose tuple is not `stress_size × 2 × nb_quad_per_facet`
//!   (upstream: throw / unchecked view); a missing (`ElementNull`) facet of
//!   a filtered element (upstream dereferences it) (UD-682, category A);
//! - a filter type without any element in the mesh (its filter is then
//!   empty) is skipped in `interpolate_stress_on_facets`: nothing to write,
//!   as upstream's loop over the empty filter, where upstream first divides
//!   by the element count 0 (UD-682, category C);
//! - a second `init_elemental_field_interpolation` with another number of
//!   points per element builds a new interpolation; upstream only resizes
//!   its matrices (`shape_functions.cc:64-81`), keeps the old point count
//!   and interpolates over partly overwritten matrices (UD-685, category C:
//!   the port's result is the correct one, no compat switch).

use akantu_core::{
    Element, ElementKind, ElementTypeMapArray, Error, GhostType, Real, Result, TypeFilter,
};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT};

use crate::SolidMechanicsModel;
use crate::material::MaterialBase;

impl MaterialBase {
    /// Upstream `Material::initElementalFieldInterpolation(points)`: build
    /// the interpolation of the material's quadrature-point fields to
    /// `points` (per `(type, ghost)`: `nb_element · nb_points_per_element`
    /// physical points, **all** elements of the type, as upstream) for the
    /// elements of the material's filter, with `fe` (the material's FE
    /// engine). Replaces a previous interpolation, also with another
    /// number of points per element (upstream reuses its matrices with the
    /// old point count, UD-685).
    ///
    /// # Errors
    /// The errors of [`FeEngine::init_elemental_field_interpolation`]
    /// (unsupported element type, non-default integration order, malformed
    /// `points`). The previous interpolation is kept on error.
    pub fn init_elemental_field_interpolation(
        &mut self,
        fe: &FeEngine,
        mesh: &Mesh,
        points: &ElementTypeMapArray<Real>,
    ) -> Result<()> {
        let interp =
            fe.init_elemental_field_interpolation(mesh, points, Some(&self.element_filter))?;
        self.stress_interpolation = Some(interp);
        Ok(())
    }

    /// Upstream `Material::interpolateStress(result, ghost_type)`: the
    /// stresses of the material's elements interpolated at the points given
    /// to [`MaterialBase::init_elemental_field_interpolation`]. `result`
    /// gets, per type, `nb_element · nb_points_per_element` tuples of
    /// `dim²` components (allocated with zeros if missing, as upstream),
    /// written for the material's elements only.
    ///
    /// # Errors
    /// No interpolation initialised (upstream: lookup throws), or the
    /// errors of [`FeEngine::interpolate_elemental_field`] (mesh changed
    /// since the initialisation, filter element not initialised, result of
    /// the wrong shape).
    pub fn interpolate_stress(
        &self,
        fe: &FeEngine,
        mesh: &Mesh,
        result: &mut ElementTypeMapArray<Real>,
        ghost: GhostType,
    ) -> Result<()> {
        let interp = self.stress_interpolation.as_ref().ok_or_else(|| {
            Error::invalid(format!(
                "interpolateStress: the elemental field interpolation of material '{}' is not \
                 initialised (call init_elemental_field_interpolation first)",
                self.name
            ))
        })?;
        // the engine takes a plain map: one copy per call, outside any loop
        let stress = self.stress.values().to_plain()?;
        fe.interpolate_elemental_field(
            mesh,
            interp,
            &stress,
            result,
            ghost,
            Some(&self.element_filter),
        )
    }

    /// Upstream `Material::interpolateStressOnFacets(result, by_elem_result,
    /// ghost_type)`: [`MaterialBase::interpolate_stress`] into
    /// `by_elem_result` (the interpolation points must be the facet
    /// quadrature points of every element, facet-major in
    /// `subelement_to_element` order), then the values copied per facet of
    /// the facet mesh into `result` (`result(facet)` is a
    /// `stress_size × 2 × nb_quad_per_facet` tensor, column-major: side 0 =
    /// the facet's first neighbour in `element_to_subelement`, side 1 =
    /// any other element). `result` must hold a block for every facet type
    /// reached (upstream does not allocate it either).
    ///
    /// Only the regular element types of the filter are visited (upstream
    /// `elementTypes(spatial_dimension, ghost_type)` defaults to
    /// `_ek_regular`). A type without any element in the mesh is skipped
    /// (its filter is empty: nothing to write; upstream divides by the
    /// element count 0 there, UD-682).
    ///
    /// # Errors
    /// No facet mesh, missing facet data or result blocks (upstream: the
    /// lookup throws), a shape mismatch, a filtered element that the mesh
    /// does not have, a missing (`ElementNull`) facet of a filtered element
    /// (UD-682), and the errors of [`MaterialBase::interpolate_stress`].
    pub fn interpolate_stress_on_facets(
        &self,
        fe: &FeEngine,
        mesh: &Mesh,
        result: &mut ElementTypeMapArray<Real>,
        by_elem_result: &mut ElementTypeMapArray<Real>,
        ghost: GhostType,
    ) -> Result<()> {
        self.interpolate_stress(fe, mesh, by_elem_result, ghost)?;
        let stress_size = self.stress.nb_component();
        let mesh_facets = mesh.mesh_facets().ok_or_else(|| {
            Error::Mesh("interpolateStressOnFacets: the mesh has no facet mesh".into())
        })?;
        let s2e = mesh_facets.data().element_map(SUBELEMENT_TO_ELEMENT)?;
        let e2s = mesh_facets.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
        // upstream `getElementFilter().elementTypes(spatial_dimension,
        // ghost_type)`: the element kind defaults to `_ek_regular`
        let tf = TypeFilter {
            spatial_dimension: Some(self.spatial_dimension),
            ghost_type: ghost,
            kind: Some(ElementKind::Regular),
        };
        let types: Vec<_> = self.element_filter.element_types(tf).collect();
        for ty in types {
            let nb_element_full = mesh.nb_element(ty, ghost);
            // upstream `by_elem_result(type, ghost_type)` throws when missing
            let by = by_elem_result.get(ty, ghost)?;
            let filter = self.element_filter.get(ty, ghost)?;
            if nb_element_full == 0 {
                // UD-682 (C): upstream divides by nb_element_full = 0 here;
                // with no element of the type the filter is empty and its
                // loop writes nothing
                if let Some(&first) = filter.as_slice().first() {
                    return Err(Error::OutOfRange {
                        what: "interpolateStressOnFacets: filtered element (the mesh has no \
                               element of its type)",
                        index: akantu_core::try_to_usize(first)?,
                        len: 0,
                    });
                }
                continue;
            }
            let nb_interpolation_points_per_elem = by.len() / nb_element_full;
            let facet_to_element = s2e.get(ty, ghost)?;
            let nb_facet_per_elem = facet_to_element.nb_component();
            if nb_facet_per_elem == 0 {
                continue;
            }
            let nb_quad_per_facet = nb_interpolation_points_per_elem / nb_facet_per_elem;
            let by = by.as_slice();
            let f2e = facet_to_element.as_slice();
            let tuple = stress_size * 2 * nb_quad_per_facet;
            for &global_el in filter.as_slice() {
                let el = akantu_core::try_to_usize(global_el)?;
                let element = Element::new(ty, global_el, ghost);
                let facets = f2e
                    .get(el * nb_facet_per_elem..(el + 1) * nb_facet_per_elem)
                    .ok_or(Error::OutOfRange {
                        what: "interpolateStressOnFacets: subelement_to_element row",
                        index: el,
                        len: f2e.len() / nb_facet_per_elem,
                    })?;
                for (f, facet) in facets.iter().enumerate() {
                    let fi = facet.try_idx().ok_or_else(|| {
                        Error::invalid(format!(
                            "interpolateStressOnFacets: facet {f} of {element} does not exist \
                             (ElementNull)"
                        ))
                    })?;
                    let first = e2s.element(*facet)?.first().and_then(|l| l.first());
                    let is_second_element = usize::from(first != Some(&element));
                    let res = result.get_mut(facet.ty(), facet.ghost_type())?;
                    if res.nb_component() != tuple {
                        return Err(Error::DimensionMismatch {
                            context: "interpolateStressOnFacets: facet result components \
                                      (stress_size × 2 × nb_quad_per_facet)",
                            expected: tuple,
                            got: res.nb_component(),
                        });
                    }
                    let rs = res.as_mut_slice();
                    let nb_facets = rs.len() / tuple.max(1);
                    let dst_el =
                        rs.get_mut(fi * tuple..(fi + 1) * tuple)
                            .ok_or(Error::OutOfRange {
                                what: "interpolateStressOnFacets: facet result",
                                index: fi,
                                len: nb_facets,
                            })?;
                    let src_base = (el * nb_interpolation_points_per_elem + f * nb_quad_per_facet)
                        * stress_size;
                    let src_el = by
                        .get(src_base..src_base + nb_quad_per_facet * stress_size)
                        .ok_or(Error::OutOfRange {
                            what: "interpolateStressOnFacets: by-element result",
                            index: el,
                            len: by.len() / (nb_interpolation_points_per_elem * stress_size).max(1),
                        })?;
                    for q in 0..nb_quad_per_facet {
                        let dst = (q * 2 + is_second_element) * stress_size;
                        dst_el[dst..dst + stress_size]
                            .copy_from_slice(&src_el[q * stress_size..(q + 1) * stress_size]);
                    }
                }
            }
        }
        Ok(())
    }
}

impl SolidMechanicsModel {
    /// Upstream `model.getMaterial(material).initElementalFieldInterpolation(points)`
    /// (SMM-024) with the model's default FE engine and mesh: see
    /// [`MaterialBase::init_elemental_field_interpolation`].
    ///
    /// # Errors
    /// An unknown material, and the errors of
    /// [`MaterialBase::init_elemental_field_interpolation`].
    pub fn init_material_elemental_field_interpolation(
        &mut self,
        material: usize,
        points: &ElementTypeMapArray<Real>,
    ) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let fe = self.physics.base.fems.get("")?;
        self.physics
            .materials
            .law_mut(material)?
            .base_mut()
            .init_elemental_field_interpolation(fe, &mesh, points)
    }

    /// Upstream `model.getMaterial(material).interpolateStress(result, ghost)`
    /// (SMM-024): see [`MaterialBase::interpolate_stress`].
    ///
    /// # Errors
    /// An unknown material, and the errors of
    /// [`MaterialBase::interpolate_stress`].
    pub fn interpolate_material_stress(
        &self,
        material: usize,
        result: &mut ElementTypeMapArray<Real>,
        ghost: GhostType,
    ) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let fe = self.physics.base.fems.get("")?;
        self.physics
            .materials
            .law(material)?
            .base()
            .interpolate_stress(fe, &mesh, result, ghost)
    }

    /// Upstream `model.getMaterial(material).interpolateStressOnFacets(result,
    /// by_elem_result, ghost)` (SMM-024): see
    /// [`MaterialBase::interpolate_stress_on_facets`].
    ///
    /// # Errors
    /// An unknown material, and the errors of
    /// [`MaterialBase::interpolate_stress_on_facets`].
    pub fn interpolate_material_stress_on_facets(
        &self,
        material: usize,
        result: &mut ElementTypeMapArray<Real>,
        by_elem_result: &mut ElementTypeMapArray<Real>,
        ghost: GhostType,
    ) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let fe = self.physics.base.fems.get("")?;
        self.physics
            .materials
            .law(material)?
            .base()
            .interpolate_stress_on_facets(fe, &mesh, result, by_elem_result, ghost)
    }
}
