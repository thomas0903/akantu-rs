// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/phase_field_model.cc
// (constructor synchronizer registration, getNbData/packData/unpackData for
// Element and Idx entities, `phase_field_model.cc:316-432`),
// src/model/phase_field/phasefield_inline_impl.hh (PhaseField::getNbData/
// packData/unpackData of `_pfm_damage`, lines 41-71),
// src/model/common/data_accessor{.hh,_tmpl.hh} (packNodalDataHelper,
// packInternalFieldHelper, packDOFDataHelper).

//! Parallel data accessors of the phase-field model (MPI-044,
//! MPI-045): what the model exchanges over the mesh's synchronizers once
//! the mesh is distributed.
//!
//! * `_constitutive_law_id` (the generic
//!   [`akantu_model::constitutive_laws_handler::LawIdAccessor`]): ghost
//!   elements receive their owner's law, appended to its ghost filter in
//!   rank-order unpack order (UD-232);
//! * `_pfm_damage` over the **element** synchronizer
//!   ([`PfmDamageAccessor`]): per law (law order, then element order), the
//!   quadrature-point `driving_force` (1), `driving_energy` (`dim`) and
//!   `damage` (1) of every element (upstream `PhaseField::packData`: three
//!   `packInternalFieldHelper` in that order); exchanged in every residual
//!   assembly between the not-ghost driving forces and the ghost assembly;
//! * `_for_dump` over the element synchronizer
//!   ([`ElementNodalDamageAccessor`]): the nodal damage of the elements'
//!   nodes in connectivity order, **unpacked** into the receiver's nodal
//!   damage (upstream `packNodalDataHelper` / `unpackNodalDataHelper`; the
//!   laws carry no data for the tag). Upstream registers the tag but never
//!   synchronises it; [`crate::PhaseFieldModel::synchronize`] offers it;
//! * `_pfm_damage` / `_for_dump` over the **node** synchronizer
//!   ([`NodalDamageAccessor`], upstream `DataAccessor<Idx>`, never
//!   registered upstream): one damage value per node, masters → slaves.

use akantu_comm::DataAccessor;
use akantu_comm::buffer::CommunicationBuffer;
use akantu_core::enums::SynchronizationTag;
use akantu_core::{Element, ElementType, ElementTypeMap, Error, GhostType, Idx, Real, Result};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::ConstitutiveLawsHandler;
use akantu_model::constitutive_laws_handler::split_by_law;

use crate::phasefield::PhaseField;

/// Bytes of one `Real`.
const R: usize = std::mem::size_of::<Real>();

/// Quadrature points per element type of the model dimension (both ghost
/// types share the rule).
pub(crate) fn nb_quad_map(fe: &FeEngine, mesh: &Mesh, dim: usize) -> Result<ElementTypeMap<usize>> {
    let mut out = ElementTypeMap::default();
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(akantu_core::TypeFilter::new().dim(dim).ghost(ghost)) {
            if out.get(ty, GhostType::NotGhost).is_none() {
                out.insert(
                    ty,
                    GhostType::NotGhost,
                    fe.nb_integration_points(ty, ghost)?,
                )?;
            }
        }
    }
    Ok(out)
}

/// `_pfm_damage` accessor of the laws (upstream `PhaseField::getNbData` /
/// `packData` / `unpackData`, reached through `splitByConstitutiveLaw`).
pub(crate) struct PfmDamageAccessor<'a> {
    pub laws: &'a mut ConstitutiveLawsHandler<dyn PhaseField>,
    pub nb_quad: &'a ElementTypeMap<usize>,
    pub dim: usize,
}

impl PfmDamageAccessor<'_> {
    fn nq(&self, ty: ElementType) -> Result<usize> {
        self.nb_quad
            .get(ty, GhostType::NotGhost)
            .copied()
            .ok_or_else(|| Error::not_found("integration points of", ty.name()))
    }

    /// Components per quadrature point of the exchanged internals, in pack
    /// order: `driving_force`, `driving_energy`, `damage`.
    fn components(&self) -> [usize; 3] {
        [1, self.dim, 1]
    }

    fn nb_data_checked(&self, elements: &[Element], tag: SynchronizationTag) -> Result<usize> {
        if tag != SynchronizationTag::PfmDamage {
            return Ok(0);
        }
        // upstream: (dim + 2) * sizeof(Real) * nb integration points, for
        // every law (empty element lists included)
        let split = split_by_law(self.laws, elements)?;
        let per_q: usize = self.components().iter().sum::<usize>() * R;
        let mut size = 0usize;
        for els in &split {
            for &(e, _) in els {
                size = size.saturating_add(self.nq(e.ty())?.saturating_mul(per_q));
            }
        }
        Ok(size)
    }
}

/// The three exchanged internals of `law` in pack order.
fn pfm_fields(law: &dyn PhaseField) -> [&akantu_model::InternalField<Real>; 3] {
    let b = law.base();
    [&b.driving_force, &b.driving_energy, &b.damage]
}

impl DataAccessor<Element> for PfmDamageAccessor<'_> {
    /// A local error (an element without law) is reported by `pack_data` /
    /// `unpack_data`, which recompute the size; the synchronizer's preflight
    /// agrees it on every rank before any message.
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        self.nb_data_checked(elements, tag).unwrap_or(usize::MAX)
    }

    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::PfmDamage {
            return Ok(());
        }
        let split = split_by_law(self.laws, elements)?;
        let comps = self.components();
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let l = self.laws.law(law)?;
            for (f, &nbc) in pfm_fields(l).into_iter().zip(&comps) {
                for &(e, local) in els {
                    let nq = self.nq(e.ty())?;
                    let a = f.get(e.ty(), e.ghost_type())?;
                    let start = local * nq * nbc;
                    let vals = a.as_slice().get(start..start + nq * nbc).ok_or_else(|| {
                        Error::invalid(format!(
                            "{e}: {} has no values (law-local {local})",
                            f.name()
                        ))
                    })?;
                    buffer.pack_slice(vals);
                }
            }
        }
        Ok(())
    }

    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::PfmDamage {
            return Ok(());
        }
        let split = split_by_law(self.laws, elements)?;
        let comps = self.components();
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let mut nqs = Vec::with_capacity(els.len());
            for &(e, _) in els {
                nqs.push(self.nq(e.ty())?);
            }
            let b = self.laws.law_mut(law)?.base_mut();
            let fields = [&mut b.driving_force, &mut b.driving_energy, &mut b.damage];
            for (f, &nbc) in fields.into_iter().zip(&comps) {
                for (&(e, local), &nq) in els.iter().zip(&nqs) {
                    let name = f.name().to_owned();
                    let mut a = f.get_mut(e.ty(), e.ghost_type())?;
                    let start = local * nq * nbc;
                    let vals = a
                        .as_mut_slice()
                        .get_mut(start..start + nq * nbc)
                        .ok_or_else(|| {
                            Error::invalid(format!("{e}: {name} has no values (law-local {local})"))
                        })?;
                    buffer.unpack_slice(vals)?;
                }
            }
        }
        Ok(())
    }
}

/// `_for_dump` over the element synchronizer: the nodal damage of the
/// elements' nodes in connectivity order (upstream `PhaseFieldModel` element
/// `packData` / `unpackData`, `packNodalDataHelper` / `unpackNodalDataHelper`:
/// the receiver overwrites its nodal damage at every node of every received
/// element, in unpack order).
pub(crate) struct ElementNodalDamageAccessor<'a> {
    pub mesh: &'a Mesh,
    /// The nodal damage (one value per node).
    pub damage: &'a mut [Real],
}

impl ElementNodalDamageAccessor<'_> {
    fn nb_nodes(elements: &[Element]) -> usize {
        elements
            .iter()
            .map(|e| e.ty().nb_nodes_per_element())
            .fold(0usize, usize::saturating_add)
    }
}

impl DataAccessor<Element> for ElementNodalDamageAccessor<'_> {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        if tag == SynchronizationTag::ForDump {
            Self::nb_nodes(elements).saturating_mul(R)
        } else {
            0
        }
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::ForDump {
            return Ok(());
        }
        for &e in elements {
            for &n in &self.mesh.element_connectivity(e)? {
                buffer.pack(node_value(self.damage, n)?);
            }
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::ForDump {
            return Ok(());
        }
        for &e in elements {
            for &n in &self.mesh.element_connectivity(e)? {
                *node_value_mut(self.damage, n)? = buffer.unpack()?;
            }
        }
        Ok(())
    }
}

/// `_pfm_damage` / `_for_dump` over the node synchronizer (upstream
/// `PhaseFieldModel` `DataAccessor<Idx>`: `packDOFDataHelper(damage)`; any
/// other tag is `AKANTU_ERROR("Unknown ghost synchronization tag")`).
pub(crate) struct NodalDamageAccessor<'a> {
    /// The nodal damage (one value per node).
    pub damage: &'a mut [Real],
}

fn node_value(damage: &[Real], n: Idx) -> Result<&Real> {
    let i = akantu_core::try_to_usize(n)?;
    damage
        .get(i)
        .ok_or_else(|| Error::invalid(format!("node {n}: no damage value")))
}

fn node_value_mut(damage: &mut [Real], n: Idx) -> Result<&mut Real> {
    let i = akantu_core::try_to_usize(n)?;
    damage
        .get_mut(i)
        .ok_or_else(|| Error::invalid(format!("node {n}: no damage value")))
}

/// Upstream `AKANTU_ERROR("Unknown ghost synchronization tag : " << tag)`.
pub(crate) fn unknown_tag(tag: SynchronizationTag) -> Error {
    Error::invalid(format!("Unknown ghost synchronization tag : {tag}"))
}

impl DataAccessor<Idx> for NodalDamageAccessor<'_> {
    fn nb_data(&self, nodes: &[Idx], tag: SynchronizationTag) -> usize {
        match tag {
            SynchronizationTag::PfmDamage | SynchronizationTag::ForDump => {
                nodes.len().saturating_mul(R)
            }
            _ => usize::MAX,
        }
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        nodes: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if !matches!(
            tag,
            SynchronizationTag::PfmDamage | SynchronizationTag::ForDump
        ) {
            return Err(unknown_tag(tag));
        }
        for &n in nodes {
            buffer.pack(node_value(self.damage, n)?);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        nodes: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if !matches!(
            tag,
            SynchronizationTag::PfmDamage | SynchronizationTag::ForDump
        ) {
            return Err(unknown_tag(tag));
        }
        for &n in nodes {
            *node_value_mut(self.damage, n)? = buffer.unpack()?;
        }
        Ok(())
    }
}
