// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion_model/diffusion_model.cc
// (constructor synchronizer registration, getNbData/packData/unpackData for
// Element and Idx entities), src/model/diffusion_model/diffusion_law.cc
// (getNbData/packData/unpackData), src/model/common/constitutive_laws/
// constitutive_laws_handler_tmpl.hh (_constitutive_law_id,
// splitByConstitutiveLaw), src/model/common/data_accessor{.hh,_tmpl.hh}
// (packNodalDataHelper, packElementalDataHelper).

//! Parallel data accessors of the diffusion model (DIFF-010): what the
//! model exchanges over the mesh's synchronizers once the mesh is
//! distributed.
//!
//! * `_constitutive_law_id` ([`LawIdAccessor`]): owners send the law index
//!   of their elements; a ghost element without a law takes the received
//!   one and is appended to that law's ghost filter in unpack order
//!   (rank order, then scheme order; UD-232 as for SMM);
//! * `_diffusion` over the **element** synchronizer ([`ElementAccessor`]):
//!   the nodal diffusion values of the elements' nodes (connectivity
//!   order), owner → ghost, unpacked into the receiver's nodal array
//!   (upstream `packNodalDataHelper` / `unpackNodalDataHelper`) — done
//!   before every internal-flow assembly so that the ghost elements'
//!   gradients use the owners' values (pure-ghost nodes have no DOF
//!   equation, the DOF manager never updates them). The laws carry no data
//!   for this tag (upstream forwards it to them and they raise
//!   `Unknown ghost synchronization tag`, UD-275; reproduced with
//!   `DiffusionUpstreamCompat::laws_reject_model_tags`);
//! * `_diffusion_gradient` over the element synchronizer: per law (law
//!   order, then element order), `∇u` of every quadrature point of the
//!   element (upstream `packElementalDataHelper`); registered upstream but
//!   never synchronised by the model (available through
//!   `DiffusionModel::synchronize`);
//! * `_diffusion` over the **node** synchronizer ([`NodalAccessor`],
//!   upstream `DataAccessor<Idx>`): one value per node, masters → slaves.

use akantu_comm::DataAccessor;
use akantu_comm::buffer::CommunicationBuffer;
use akantu_core::array_shared::SharedArray;
use akantu_core::enums::SynchronizationTag;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMap, Error, GhostType, Idx, Real, Result,
};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::ConstitutiveLawsHandler;

use crate::law::{DiffusionLaw, DiffusionUpstreamCompat, unknown_tag};

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

/// `_constitutive_law_id` accessor (upstream `ConstitutiveLawsHandler`
/// pack/unpack for that tag).
pub(crate) struct LawIdAccessor<'a> {
    pub laws: &'a mut ConstitutiveLawsHandler<dyn DiffusionLaw>,
}

impl DataAccessor<Element> for LawIdAccessor<'_> {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        if tag == SynchronizationTag::ConstitutiveLawId {
            elements.len().saturating_mul(std::mem::size_of::<Idx>())
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
        if tag != SynchronizationTag::ConstitutiveLawId {
            return Ok(());
        }
        let map = self.laws.law_index_map();
        for &e in elements {
            buffer.pack(&map.element_ref(e)?[0]);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::ConstitutiveLawId {
            return Ok(());
        }
        for &e in elements {
            let recv: Idx = buffer.unpack()?;
            let current = self.laws.law_index_map().element_ref(e)?[0];
            if current != -1 {
                continue;
            }
            let law = akantu_core::decode_idx(recv)?.ok_or_else(|| {
                Error::Communication(format!("{e}: received no constitutive law from its owner"))
            })?;
            let local = self.laws.law_mut(law)?.add_element(e)?;
            self.laws.law_index_map_mut().set_element(e, &[recv])?;
            self.laws.local_numbering_map_mut().element_mut(e)?[0] = local;
        }
        Ok(())
    }
}

/// Elements of `elements` split by law (upstream `splitByConstitutiveLaw`):
/// for every law in order, `(element, law-local index)` in input order.
pub(crate) fn split_by_law(
    laws: &ConstitutiveLawsHandler<dyn DiffusionLaw>,
    elements: &[Element],
) -> Result<Vec<Vec<(Element, usize)>>> {
    let mut out: Vec<Vec<(Element, usize)>> = vec![Vec::new(); laws.nb_laws()];
    for &e in elements {
        let li = laws.law_index_map().element_ref(e)?[0];
        let law = akantu_core::decode_idx(li)?
            .ok_or_else(|| Error::invalid(format!("{e} has no constitutive law")))?;
        let local = akantu_core::try_to_usize(laws.local_numbering_map().element(e)?[0])?;
        out.get_mut(law)
            .ok_or_else(|| Error::invalid(format!("{e}: law {law} out of range")))?
            .push((e, local));
    }
    Ok(out)
}

/// Element accessor of the model (upstream `DiffusionModel` element
/// `getNbData` / `packData` / `unpackData`, which append the handler's —
/// i.e. the laws' — data).
pub(crate) struct ElementAccessor<'a> {
    pub mesh: &'a Mesh,
    pub laws: &'a mut ConstitutiveLawsHandler<dyn DiffusionLaw>,
    pub nb_quad: &'a ElementTypeMap<usize>,
    /// The nodal diffusion field, lent by the DOF manager for the exchange
    /// (written by `_diffusion` unpacks through a write guard per call).
    pub diffusion: SharedArray<Real>,
    pub compat: DiffusionUpstreamCompat,
}

impl ElementAccessor<'_> {
    fn nq(&self, ty: ElementType) -> usize {
        self.nb_quad
            .get(ty, GhostType::NotGhost)
            .copied()
            .unwrap_or(0)
    }

    fn laws_nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> Result<usize> {
        let split = split_by_law(self.laws, elements)?;
        let nq = |ty: ElementType| self.nq(ty);
        let mut size = 0usize;
        for (i, els) in split.iter().enumerate() {
            // upstream calls every law, also with an empty element list
            size = size.saturating_add(self.laws.law(i)?.nb_data(&nq, els, tag, self.compat)?);
        }
        Ok(size)
    }

    fn nb_data_checked(&self, elements: &[Element], tag: SynchronizationTag) -> Result<usize> {
        let mut size = 0usize;
        if tag == SynchronizationTag::Diffusion {
            let nb_nodes: usize = elements.iter().map(|e| e.ty().nb_nodes_per_element()).sum();
            size = nb_nodes.saturating_mul(R);
        }
        Ok(size.saturating_add(self.laws_nb_data(elements, tag)?))
    }
}

impl DataAccessor<Element> for ElementAccessor<'_> {
    /// A local error (UD-275 compat: a law refusing the tag; an element
    /// without law) is reported by `pack_data` / `unpack_data`, which
    /// recompute the size: the synchronizer's preflight agrees the packing
    /// failure on every rank before any message, with upstream's message.
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        self.nb_data_checked(elements, tag).unwrap_or(0)
    }

    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        // the size computation carries the laws' verdict (UD-275)
        self.nb_data_checked(elements, tag)?;
        if tag == SynchronizationTag::Diffusion {
            let u = self.diffusion.read()?;
            for &e in elements {
                for &n in &self.mesh.element_connectivity(e)? {
                    buffer.pack_slice(u.get_tuple(akantu_core::try_to_usize(n)?)?);
                }
            }
        }
        if tag == SynchronizationTag::DiffusionGradient {
            let split = split_by_law(self.laws, elements)?;
            for (i, els) in split.iter().enumerate() {
                let law = self.laws.law(i)?;
                let d = law.base().spatial_dimension;
                for &(e, local) in els {
                    let nq = self.nq(e.ty());
                    let a = law.base().grad_u.get(e.ty(), e.ghost_type())?;
                    let start = local * nq * d;
                    let vals = a.as_slice().get(start..start + nq * d).ok_or_else(|| {
                        Error::invalid(format!("{e}: ∇u has no values (law-local {local})"))
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
        self.nb_data_checked(elements, tag)?;
        if tag == SynchronizationTag::Diffusion {
            let mut u = self.diffusion.write()?;
            for &e in elements {
                for &n in &self.mesh.element_connectivity(e)? {
                    let t = u.get_tuple_mut(akantu_core::try_to_usize(n)?)?;
                    buffer.unpack_slice(t)?;
                }
            }
        }
        if tag == SynchronizationTag::DiffusionGradient {
            let split = split_by_law(self.laws, elements)?;
            for (i, els) in split.iter().enumerate() {
                let mut nqs = Vec::with_capacity(els.len());
                for &(e, _) in els {
                    nqs.push(self.nq(e.ty()));
                }
                let law = self.laws.law_mut(i)?;
                let d = law.base().spatial_dimension;
                for (&(e, local), &nq) in els.iter().zip(&nqs) {
                    let mut a = law.base_mut().grad_u.get_mut(e.ty(), e.ghost_type())?;
                    let start = local * nq * d;
                    let vals =
                        a.as_mut_slice()
                            .get_mut(start..start + nq * d)
                            .ok_or_else(|| {
                                Error::invalid(format!("{e}: ∇u has no values (law-local {local})"))
                            })?;
                    buffer.unpack_slice(vals)?;
                }
            }
        }
        Ok(())
    }
}

/// DOF-entity accessor of the model (upstream `getNbData/packData/
/// unpackData(Array<Idx>)`): `_diffusion` packs `u[node]`; any other tag is
/// upstream's `AKANTU_ERROR("Unknown ghost synchronization tag")`.
pub(crate) struct NodalAccessor<'a> {
    pub diffusion: &'a mut Array<Real>,
}

impl DataAccessor<Idx> for NodalAccessor<'_> {
    fn nb_data(&self, nodes: &[Idx], tag: SynchronizationTag) -> usize {
        if tag == SynchronizationTag::Diffusion {
            nodes.len().saturating_mul(R)
        } else {
            usize::MAX
        }
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        nodes: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::Diffusion {
            return Err(unknown_tag(tag));
        }
        for &n in nodes {
            buffer.pack_slice(self.diffusion.get_tuple(akantu_core::try_to_usize(n)?)?);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        nodes: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::Diffusion {
            return Err(unknown_tag(tag));
        }
        for &n in nodes {
            buffer.unpack_slice(
                self.diffusion
                    .get_tuple_mut(akantu_core::try_to_usize(n)?)?,
            )?;
        }
        Ok(())
    }
}

impl super::DiffusionModel {
    /// Upstream `SynchronizerRegistry::synchronize(tag)` for the tags the
    /// model registers on the mesh's element synchronizer (`_diffusion`,
    /// `_diffusion_gradient`, `_constitutive_law_id`): one collective
    /// exchange owner → ghost. Nothing on a mesh without element
    /// synchronizer (serial). Another tag is an error (upstream: no
    /// synchronizer registered, nothing happens).
    pub fn synchronize(&mut self, tag: SynchronizationTag) -> Result<()> {
        use SynchronizationTag as T;
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let Some(es) = &mesh.synchronizers().element else {
            return Ok(());
        };
        match tag {
            T::Diffusion | T::DiffusionGradient => {
                let fe = self.physics.base.fems.get("")?;
                let nb_quad = nb_quad_map(fe, &mesh, self.physics.base.spatial_dimension)?;
                let name = self.physics.dof_name.clone();
                let u = {
                    let f = self
                        .solver
                        .dof_manager
                        .family_mut(&name)
                        .map_err(|_| super::not_allocated(&name))?;
                    // lent to the accessor for the exchange (moving the
                    // `SharedArray` keeps its allocation and any export)
                    std::mem::replace(&mut f.dofs, SharedArray::new(Array::new(1)))
                };
                let mut acc = ElementAccessor {
                    mesh: &mesh,
                    laws: &mut self.physics.laws,
                    nb_quad: &nb_quad,
                    diffusion: u,
                    compat: self.physics.upstream_compat,
                };
                let r = es.synchronize(&mut acc, tag);
                let u = acc.diffusion;
                self.solver
                    .dof_manager
                    .family_mut(&name)
                    .map_err(|_| super::not_allocated(&name))?
                    .dofs = u;
                r
            }
            T::ConstitutiveLawId => {
                let mut acc = LawIdAccessor {
                    laws: &mut self.physics.laws,
                };
                es.synchronize(&mut acc, tag)
            }
            // UD-553 compat: upstream's registry iterates over no
            // synchronizer for the tag (`_clh_init_cl` included, UD-900)
            _ if self
                .physics
                .upstream_compat
                .unregistered_synchronization_tag_is_a_noop =>
            {
                Ok(())
            }
            other => Err(Error::invalid(format!(
                "DiffusionModel::synchronize: tag {other} is not registered by the model"
            ))),
        }
    }

    /// Upstream DOF-entity data accessor of the model driven by the mesh's
    /// **node** synchronizer: `_diffusion` (masters overwrite their slaves'
    /// values). Collective; nothing on a mesh without node synchronizer.
    pub fn synchronize_nodal(&mut self, tag: SynchronizationTag) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let Some(ns) = &mesh.synchronizers().node else {
            return Ok(());
        };
        if tag != SynchronizationTag::Diffusion {
            return Err(unknown_tag(tag));
        }
        let name = self.physics.dof_name.clone();
        let dm = &mut self.solver.dof_manager;
        let mut u = {
            let g = dm.dofs(&name)?;
            Array::clone(&g)
        };
        {
            let mut acc = NodalAccessor { diffusion: &mut u };
            ns.synchronize(&mut acc, tag)?;
        }
        dm.dofs_mut(&name)?.copy_from(&u)
    }
}
