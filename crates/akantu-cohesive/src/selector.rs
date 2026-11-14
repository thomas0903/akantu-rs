// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_cohesive/material_selector_cohesive.{hh,cc}.

//! Cohesive material selectors (COH-009): `DefaultMaterialCohesiveSelector`
//! (a cohesive element takes the material of its original facet),
//! `MeshDataMaterialCohesiveSelector` (by the facet's physical name) and
//! `MaterialCohesiveRulesSelector` (by the pair of names of the facet's
//! neighbours).

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use akantu_core::{Element, ElementKind, ElementTypeMapArray, Idx, Result};
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, PHYSICAL_NAMES, SUBELEMENT_TO_ELEMENT};
use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};

/// Upstream `DefaultMaterialCohesiveSelector`: cohesive element → the
/// `facet_material` of its original facet (`subelement_to_element` side 0,
/// side 1 in 3-D), else the first cohesive material; facet → its
/// `facet_material`; anything else → the next selector of the chain.
#[derive(Clone, Debug, Default)]
pub struct DefaultMaterialCohesiveSelector {
    /// The model's `facet_material` and first cohesive material, shared
    /// with the model (always current).
    pub state: Arc<RwLock<CohesiveSelectorState>>,
}

/// State read by the cohesive selectors (owned by the model).
#[derive(Clone, Debug, Default)]
pub struct CohesiveSelectorState {
    /// The model's `facet_material`.
    pub facet_material: ElementTypeMapArray<Idx>,
    /// Index of the first cohesive material (upstream fallback cohesive
    /// value; `None` = `-1`).
    pub fallback_cohesive: Option<usize>,
}

fn decode(v: Idx) -> Option<usize> {
    usize::try_from(v).ok()
}

impl DefaultMaterialCohesiveSelector {
    fn select_default(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        let st = self
            .state
            .read()
            .map_err(|_| akantu_core::Error::invalid("cohesive selector state poisoned"))?;
        let dim = ctx.mesh.spatial_dimension();
        if element.kind() == ElementKind::Cohesive {
            let third = usize::from(dim == 3);
            let facet = ctx
                .mesh
                .mesh_facets()
                .and_then(|fm| fm.data().element_map(SUBELEMENT_TO_ELEMENT).ok())
                .and_then(|m| m.element(element).ok().and_then(|r| r.get(third).copied()));
            let Some(facet) = facet else {
                return Ok(st.fallback_cohesive);
            };
            if st.facet_material.exists(facet.ty(), facet.ghost_type()) {
                return Ok(st
                    .facet_material
                    .element(facet)
                    .ok()
                    .and_then(|v| v.first().copied())
                    .and_then(decode)
                    .or(st.fallback_cohesive));
            }
            return Ok(st.fallback_cohesive);
        }
        if element.ty().spatial_dimension() + 1 == dim {
            return Ok(st
                .facet_material
                .element(element)
                .ok()
                .and_then(|v| v.first().copied())
                .and_then(decode));
        }
        Ok(None)
    }
}

impl ConstitutiveLawSelector for DefaultMaterialCohesiveSelector {
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        self.select_default(ctx, element)
    }
}

/// Upstream `MeshDataMaterialCohesiveSelector`: cohesive elements and facets
/// take the material named by the facet's `physical_names` entry, else
/// the default cohesive selection.
#[derive(Clone, Debug)]
pub struct MeshDataMaterialCohesiveSelector {
    /// The default cohesive selector (fallback).
    pub default: DefaultMaterialCohesiveSelector,
}

impl ConstitutiveLawSelector for MeshDataMaterialCohesiveSelector {
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        let dim = ctx.mesh.spatial_dimension();
        if element.kind() == ElementKind::Cohesive || element.ty().spatial_dimension() + 1 == dim {
            let fm = ctx.mesh.mesh_facets();
            let facet = if element.kind() == ElementKind::Cohesive {
                fm.and_then(|fm| fm.data().element_map(SUBELEMENT_TO_ELEMENT).ok())
                    .and_then(|m| m.element(element).ok())
                    .and_then(|r| r.get(usize::from(dim == 3)).copied())
            } else {
                Some(element)
            };
            let name = facet.and_then(|f| {
                fm.and_then(|fm| fm.data().strings(PHYSICAL_NAMES).ok())
                    .and_then(|n| n.element(f).ok().and_then(|v| v.first().cloned()))
            });
            if let Some(i) = name.and_then(|n| (ctx.law_by_name)(&n)) {
                return Ok(Some(i));
            }
            return self.default.select_default(ctx, element);
        }
        self.default.select_default(ctx, element)
    }
}

/// Upstream `MaterialCohesiveRules`: `(name of side 1, name of side 2)` →
/// material name.
pub type MaterialCohesiveRules = BTreeMap<(String, String), String>;

/// Upstream `MaterialCohesiveRulesSelector`: a facet whose two neighbours
/// carry the mesh-data names `(a, b)` (or `(b, a)`) of a rule gets the
/// rule's material; a boundary facet (no second neighbour) is looked up as
/// `(a, a)`; otherwise the default cohesive selection. Cohesive elements
/// take their facet's material (the default cohesive selection); bulk
/// elements defer to the next selector of the chain (upstream's own
/// fallback chain ends with `MeshDataMaterialSelector(mesh_data_id)`: put
/// an [`akantu_model::constitutive_law::ElementDataSelector`] after this
/// selector, as upstream's `cohesive_extrinsic_ig_tg` example does).
///
/// As upstream, a neighbour without a `mesh_data_id` entry and a rule
/// naming no material of the model are errors (upstream `Mesh::getData`
/// and `getMaterialIndex` throw).
#[derive(Clone, Debug)]
pub struct MaterialCohesiveRulesSelector {
    /// Rules.
    pub rules: MaterialCohesiveRules,
    /// Element data holding the names (upstream `mesh_data_id`,
    /// `physical_names` by default).
    pub mesh_data_id: String,
    /// Fallback.
    pub default: DefaultMaterialCohesiveSelector,
}

impl ConstitutiveLawSelector for MaterialCohesiveRulesSelector {
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        let dim = ctx.mesh.spatial_dimension();
        if element.ty().spatial_dimension() + 1 == dim
            && element.kind() == ElementKind::Regular
            && let Some(fm) = ctx.mesh.mesh_facets()
        {
            let row = fm
                .data()
                .element_lists(ELEMENT_TO_SUBELEMENT)?
                .element(element)?[0]
                .clone();
            let names = ctx.mesh.data().strings(&self.mesh_data_id)?;
            let name_of = |e: Element| -> Result<String> {
                names
                    .element(e)
                    .ok()
                    .and_then(|v| v.first().cloned())
                    .ok_or_else(|| {
                        akantu_core::Error::not_found(
                            "element data entry",
                            format!("{} of {e}", self.mesh_data_id),
                        )
                    })
            };
            let el1 = row.first().copied().unwrap_or(Element::NULL);
            if el1.is_null() {
                return Err(akantu_core::Error::Mesh(format!(
                    "facet {element} has no first neighbour"
                )));
            }
            let id1 = name_of(el1)?;
            let id2 = match row.get(1) {
                Some(e) if !e.is_null() => name_of(*e)?,
                _ => id1.clone(),
            };
            let rule = self
                .rules
                .get(&(id1.clone(), id2.clone()))
                .or_else(|| self.rules.get(&(id2, id1)));
            if let Some(r) = rule {
                return (ctx.law_by_name)(r)
                    .map(Some)
                    .ok_or_else(|| akantu_core::Error::not_found("material", r.as_str()));
            }
        }
        self.default.select_default(ctx, element)
    }
}
