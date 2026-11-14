// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/solid_mechanics_model_igfem.cc
// (SolidMechanicsModelIGFEM::transferInternalValues), material_igfem/material_igfem.cc
// (MaterialIGFEM::extrapolateInternal), material_igfem/material_igfem_saw_tooth_damage.cc
// (the copy-back of MaterialIGFEMSawToothDamage::onElementsAdded), and
// src/model/solid_mechanics/material.cc (Material::extrapolateInternal).

//! Transfer of internals on an interface update (IGFEM-004 / 005).
//!
//! Upstream: when an interface update adds elements to an IGFEM damage
//! law, the law's `onElementsAdded` asks the model for the values of each
//! internal of `internals_to_transfer` at its new integration points
//! (`SolidMechanicsModelIGFEM::transferInternalValues`): for every new
//! element, the **replaced** element (`element_map`, filled from the
//! `NewIGFEMElementsEvent`) and its law (`material_index`) give
//! `old_material.extrapolateInternal(internal, old_element, points, out)` —
//! not a real extrapolation: the value of the old element's first
//! integration point whose norm is positive (`Material`) or above the
//! tolerance (`MaterialIGFEM`), copied to every new point; zeros when the
//! old law has no such internal. The law copies the result into the points
//! of its **damageable** sub-material (`sub_material ≠ 0`) only.
//!
//! akantu-rs dispatches a mutation's events after the commit, in recorded
//! order: the replaced elements are erased (`ElementsRemoved`, the laws
//! compact their internals) before the new ones are added (`ElementsAdded`,
//! D-095 rule 2). The IGFEM model's handler therefore captures, on
//! `ElementsRemoved` and **before** the laws compact, the values of the
//! internals that some IGFEM law transfers ([`TransferSnapshot::capture`]),
//! and applies the transfer on `ElementsAdded` after the laws received the
//! new elements ([`transfer_internals`]). The old → new map is the
//! `NewIgfemElements` extension of the event (UD-1204).

use std::collections::{BTreeMap, BTreeSet};

use akantu_core::{Element, Error, Real, Result};
use akantu_model::constitutive_law::InternalFieldMut;
use akantu_model::constitutive_laws_handler::ConstitutiveLawsHandler;
use akantu_solid::material::Material;

use crate::material::{as_igfem, as_igfem_mut, extrapolate_first_point, law_element_values};

/// The state of one replaced element: its law, the threshold of its law's
/// `extrapolateInternal`, the captured internals `(nb_component, values at
/// its points)`.
#[derive(Clone, Debug)]
pub struct ReplacedElement {
    /// Law index of the element before the update (upstream
    /// `material_index(old_element)`).
    pub law: usize,
    /// `0` for a regular law (`Material::extrapolateInternal`: norm `> 0`),
    /// the law's tolerance for an IGFEM law (`MaterialIGFEM`'s: norm `>
    /// Math::getTolerance()`).
    pub threshold: Real,
    /// Captured `Real` internals by name.
    pub internals: BTreeMap<String, (usize, Vec<Real>)>,
}

/// The replaced elements' internals, captured on `ElementsRemoved` before
/// the laws compact them.
#[derive(Clone, Debug, Default)]
pub struct TransferSnapshot {
    /// Replaced element (numbering before the update) → its state.
    pub elements: BTreeMap<Element, ReplacedElement>,
}

/// The internals that the IGFEM laws of `materials` transfer (the union of
/// their `internals_to_transfer`).
#[must_use]
pub fn transferred_internals(
    materials: &ConstitutiveLawsHandler<dyn Material>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for law in materials.laws() {
        if let Some(m) = as_igfem(law.as_ref()) {
            names.extend(m.internals_to_transfer().iter().map(|s| (*s).to_owned()));
        }
    }
    names
}

impl TransferSnapshot {
    /// Capture the internals `names` of every element of `elements` that
    /// has a law, before an `ElementsRemoved` is applied to the laws.
    ///
    /// # Errors
    /// Inconsistent law maps or internals.
    pub fn capture(
        materials: &ConstitutiveLawsHandler<dyn Material>,
        elements: &[Element],
        names: &BTreeSet<String>,
    ) -> Result<Self> {
        let mut out = Self::default();
        if names.is_empty() {
            return Ok(out);
        }
        let law_map = materials.law_index_map();
        for &el in elements {
            let Ok(row) = law_map.element_ref(el) else {
                continue;
            };
            let Ok(law) = usize::try_from(row[0]) else {
                continue; // no law (upstream: not in any material)
            };
            let l = materials.law(law)?;
            let threshold = as_igfem(l).map_or(0.0, |m| m.igfem_base().tolerance);
            let mut internals = BTreeMap::new();
            for name in names {
                if let Some(v) = law_element_values(l, name, el)? {
                    internals.insert(name.clone(), v);
                }
            }
            out.elements.insert(
                el,
                ReplacedElement {
                    law,
                    threshold,
                    internals,
                },
            );
        }
        Ok(out)
    }

    /// Upstream `old_material.extrapolateInternal(id, old_element, …)` for
    /// `nq` new points of `nb_component` components: the replaced element's
    /// first point above its law's threshold, copied to every point; zeros
    /// when its law has no internal `id` (or the element had no law).
    ///
    /// # Errors
    /// A component count that differs from the old internal's.
    pub fn extrapolate(
        &self,
        old: Element,
        id: &str,
        nb_component: usize,
        nq: usize,
    ) -> Result<Vec<Real>> {
        let mut out = vec![0.0; nq * nb_component];
        let Some(r) = self.elements.get(&old) else {
            return Ok(out);
        };
        let Some((nc, vals)) = r.internals.get(id) else {
            return Ok(out);
        };
        if *nc != nb_component {
            return Err(Error::DimensionMismatch {
                context: "transferInternalValues: components of the old and new internal",
                expected: nb_component,
                got: *nc,
            });
        }
        extrapolate_first_point(vals, *nc, r.threshold, &mut out)?;
        Ok(out)
    }
}

/// Upstream `transferInternalValues` and the copy-back of the IGFEM damage
/// laws' `onElementsAdded`, for the elements `new_elements` of one
/// `ElementsAdded` event of an interface update (old elements in
/// `element_map`, new → old): every IGFEM law with internals to transfer
/// receives, at the damageable points (`sub_material ≠ 0`) of its new
/// elements, the extrapolated values of the replaced elements; then its
/// `after_transfer` hook.
///
/// # Errors
/// A new IGFEM element of a transferring law without a replaced element,
/// inconsistent maps or blocks.
pub fn transfer_internals(
    materials: &mut ConstitutiveLawsHandler<dyn Material>,
    snapshot: &TransferSnapshot,
    element_map: &BTreeMap<Element, Element>,
    new_elements: &[Element],
) -> Result<()> {
    for law in 0..materials.nb_laws() {
        let names = match as_igfem(materials.law(law)?) {
            Some(m) if !m.internals_to_transfer().is_empty() => m.internals_to_transfer(),
            _ => continue,
        };
        // the new elements of this law and their law-local index
        let mut mine = Vec::new();
        {
            let law_map = materials.law_index_map();
            let local = materials.local_numbering_map();
            for &el in new_elements {
                let Ok(row) = law_map.element_ref(el) else {
                    continue;
                };
                if usize::try_from(row[0]).ok() != Some(law) {
                    continue;
                }
                let loc = usize::try_from(local.element(el)?[0])
                    .map_err(|_| Error::invalid(format!("{el}: no law-local index in its law")))?;
                let old = *element_map.get(&el).ok_or_else(|| {
                    Error::invalid(format!(
                        "transferInternalValues: new element {el} has no replaced element in the \
                         update's element map"
                    ))
                })?;
                mine.push((el, loc, old));
            }
        }
        if mine.is_empty() {
            continue;
        }
        let l = materials.law_mut(law)?;
        let ctx = as_igfem(l)
            .ok_or_else(|| Error::invalid("IGFEM law expected"))?
            .igfem_base()
            .context()
            .clone();
        for &name in names {
            // the values to write: (element, local, extrapolated, sub_material)
            let mut writes = Vec::with_capacity(mine.len());
            let base = as_igfem(l)
                .ok_or_else(|| Error::invalid("IGFEM law expected"))?
                .igfem_base();
            let mut nc_of = None;
            l.for_each_internal(&mut |n, f| {
                if n == name
                    && let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f
                {
                    nc_of = Some(f.nb_component());
                }
            });
            let nc = nc_of.ok_or_else(|| {
                Error::not_found("internal to transfer", format!("{name} of {}", l.law_id()))
            })?;
            for &(el, loc, old) in &mine {
                let nq = ctx.engine()?.nb_integration_points(el.ty())?;
                let vals = snapshot.extrapolate(old, name, nc, nq)?;
                let sub = base.sub_material.get(el.ty(), el.ghost_type())?;
                let s = sub
                    .as_slice()
                    .get(loc * nq..(loc + 1) * nq)
                    .ok_or_else(|| Error::invalid(format!("{el}: sub_material block too short")))?
                    .to_vec();
                writes.push((el, loc, nq, vals, s));
            }
            l.for_each_internal_mut(&mut |n, f| {
                if n != name {
                    return Ok(());
                }
                let InternalFieldMut::Real(f) = f else {
                    return Ok(());
                };
                for (el, loc, nq, vals, sub) in &writes {
                    let mut block = f.get_mut(el.ty(), el.ghost_type())?;
                    let dst = block
                        .as_mut_slice()
                        .get_mut(loc * nq * nc..(loc + 1) * nq * nc)
                        .ok_or_else(|| Error::invalid(format!("{el}: {name} block too short")))?;
                    for q in 0..*nq {
                        if sub[q] != 0 {
                            dst[q * nc..(q + 1) * nc].copy_from_slice(&vals[q * nc..(q + 1) * nc]);
                        }
                    }
                }
                Ok(())
            })?;
        }
        if let Some(m) = as_igfem_mut(l) {
            m.after_transfer()?;
        }
    }
    Ok(())
}
