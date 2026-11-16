// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/solid_mechanics_model.cc
// (constructor synchronizer registration, getNbData/packData/unpackData for
// Element and DOF entities), src/model/solid_mechanics/material_inline_impl.hh
// (Material::getNbData/packData/unpackData for _smm_stress/_smm_gradu),
// src/model/common/constitutive_laws/constitutive_laws_handler_tmpl.hh
// (assignConstitutiveLawToElements: _constitutive_law_id, splitByConstitutiveLaw),
// src/model/common/data_accessor{.hh,_tmpl.hh} (pack/unpack helpers).

//! Parallel data accessors of the solid mechanics model (SMM-016,
//! MODEL-011, MPI-026…036): what the model exchanges over the mesh's
//! element synchronizer once the mesh is distributed.
//!
//! * `_constitutive_law_id` ([`LawIdAccessor`]): owners send the law index
//!   of their elements; a ghost element without a law takes the received
//!   one and is appended to that law's ghost filter **in unpack order**
//!   (receive schemes in rank order, then scheme order — upstream unpacks
//!   in `MPI_Waitany` completion order, which equals rank order when the
//!   messages are already there; UD-232, documented in NUMERICAL_VALIDATION);
//! * `_smm_stress` / `_smm_gradu` ([`StressAccessor`]): per law (law
//!   order, then element order — upstream `splitByConstitutiveLaw`), the
//!   quadrature-point values of the law's internals: finite deformation
//!   `piola_kirchhoff_2`, `grad_u`, then `stress`; small strain `stress`
//!   (`_smm_stress`), or `grad_u` (`_smm_gradu`);
//! * model element tags `_smm_mass`, `_smm_for_gradu`, `_smm_boundary`,
//!   `_for_dump` ([`ElementNodalAccessor`]): upstream packs the nodal values
//!   of the elements' nodes in connectivity order but its element
//!   `unpackData` only delegates to the materials, so the values are never
//!   unpacked (UD-233; the typed wire contract requires every byte to
//!   be consumed: the port reads and discards them — same observable
//!   state);
//! * model DOF tags `_smm_uv`, `_smm_res`, `_smm_mass`, `_for_dump`
//!   ([`NodalAccessor`]): per node `dim` values of each array (in that
//!   order), packed and unpacked.
//!
//! Exchanges inside a solver callback go through the running operation's
//! scope (`DofManager::phases`, ARCHITECTURE.md, "DOF manager"); stand-alone ones (law assignment at
//! initialisation) own their scope.

use akantu_comm::DataAccessor;
use akantu_comm::buffer::CommunicationBuffer;
use akantu_core::enums::SynchronizationTag;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMap, Error, GhostType, Idx, Real, Result,
};
use akantu_mesh::Mesh;
use akantu_model::ConstitutiveLawsHandler;

use crate::material::Material;

/// Bytes of one `Real`.
const R: usize = std::mem::size_of::<Real>();

/// `_constitutive_law_id` accessor (upstream `ConstitutiveLawsHandler`
/// pack/unpack for that tag).
pub(crate) struct LawIdAccessor<'a> {
    pub materials: &'a mut ConstitutiveLawsHandler<dyn Material>,
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
        let map = self.materials.law_index_map();
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
            let current = self.materials.law_index_map().element_ref(e)?[0];
            if current != -1 {
                continue;
            }
            let law = akantu_core::decode_idx(recv)?.ok_or_else(|| {
                Error::Communication(format!("{e}: received no constitutive law from its owner"))
            })?;
            let local = self.materials.law_mut(law)?.add_element(e)?;
            self.materials.law_index_map_mut().set_element(e, &[recv])?;
            self.materials.local_numbering_map_mut().element_mut(e)?[0] = local;
        }
        Ok(())
    }
}

/// Elements of `elements` split by law (upstream `splitByConstitutiveLaw`):
/// for every law in order, `(element, law-local index)` in input order.
///
/// The law index and law-local numbering blocks are looked up once per run
/// of consecutive elements of one `(type, ghost)` (the synchronizer schemes
/// are sorted that way), not once per element.
fn split_by_law(
    materials: &ConstitutiveLawsHandler<dyn Material>,
    elements: &[Element],
) -> Result<Vec<Vec<(Element, usize)>>> {
    let nb_laws = materials.nb_laws();
    let mut out: Vec<Vec<(Element, usize)>> = vec![Vec::new(); nb_laws];
    if let [only] = out.as_mut_slice() {
        only.reserve_exact(elements.len());
    }
    let law_index = materials.law_index_map();
    let numbering = materials.local_numbering_map();
    for run in type_runs(elements, |e| *e) {
        let (ty, ghost) = (run[0].ty(), run[0].ghost_type());
        let laws = law_index.get(ty, ghost)?;
        let locals = numbering.get(ty, ghost)?;
        let (lnc, nnc) = (laws.nb_component(), locals.nb_component());
        let (laws, locals) = (laws.as_slice(), locals.as_slice());
        for &e in run {
            let i = e
                .try_idx()
                .ok_or_else(|| Error::invalid("ElementTypeMapArray::element: ElementNull"))?;
            let li = *laws.get(i * lnc).ok_or(Error::OutOfRange {
                what: "element",
                index: i,
                len: laws.len() / lnc.max(1),
            })?;
            let law = akantu_core::decode_idx(li)?
                .ok_or_else(|| Error::invalid(format!("{e} has no constitutive law")))?;
            let local = *locals.get(i * nnc).ok_or(Error::OutOfRange {
                what: "element",
                index: i,
                len: locals.len() / nnc.max(1),
            })?;
            let local = akantu_core::try_to_usize(local)?;
            out.get_mut(law)
                .ok_or_else(|| Error::invalid(format!("{e}: law {law} out of range")))?
                .push((e, local));
        }
    }
    Ok(out)
}

/// The maximal runs of consecutive items whose elements (`key`) share one
/// `(type, ghost)` block, in order.
fn type_runs<T>(items: &[T], key: impl Fn(&T) -> Element) -> impl Iterator<Item = &[T]> {
    let mut rest = items;
    std::iter::from_fn(move || {
        let first = key(rest.first()?);
        let (ty, ghost) = (first.ty(), first.ghost_type());
        let n = rest
            .iter()
            .position(|x| {
                let e = key(x);
                e.ty() != ty || e.ghost_type() != ghost
            })
            .unwrap_or(rest.len());
        let (run, tail) = rest.split_at(n);
        rest = tail;
        Some(run)
    })
}

/// `_smm_stress` / `_smm_gradu` accessor (upstream `Material` pack/unpack,
/// `packInternalFieldHelper`: the quadrature-point values of each element
/// of the law, element by element).
pub(crate) struct StressAccessor<'a> {
    pub materials: &'a mut ConstitutiveLawsHandler<dyn Material>,
    /// Quadrature points per element type (the model's FE engine).
    pub nb_quad: &'a ElementTypeMap<usize>,
    pub dim: usize,
}

impl StressAccessor<'_> {
    fn nq(&self, ty: ElementType) -> Result<usize> {
        self.nb_quad
            .get(ty, GhostType::NotGhost)
            .copied()
            .ok_or_else(|| Error::not_found("integration points of", ty.name()))
    }

    /// The internals exchanged for `tag` by `law`, in upstream order.
    fn fields(law: &dyn Material, tag: SynchronizationTag) -> Vec<&'static str> {
        match tag {
            SynchronizationTag::SmmStress if law.base().finite_deformation => {
                vec!["piola_kirchhoff_2", "grad_u", "stress"]
            }
            SynchronizationTag::SmmStress => vec!["stress"],
            SynchronizationTag::SmmGradu => vec!["grad_u"],
            _ => Vec::new(),
        }
    }
}

fn field<'m>(
    law: &'m dyn Material,
    name: &str,
) -> Result<&'m akantu_model::internal_field::InternalField<Real>> {
    let b = law.base();
    match name {
        "stress" => Ok(&b.stress),
        "grad_u" => Ok(&b.grad_u),
        "piola_kirchhoff_2" => b
            .piola_kirchhoff_2
            .as_ref()
            .ok_or_else(|| Error::invalid("piola_kirchhoff_2 is not registered")),
        _ => Err(Error::not_found("internal", name)),
    }
}

fn field_mut<'m>(
    law: &'m mut dyn Material,
    name: &str,
) -> Result<&'m mut akantu_model::internal_field::InternalField<Real>> {
    let b = law.base_mut();
    match name {
        "stress" => Ok(&mut b.stress),
        "grad_u" => Ok(&mut b.grad_u),
        "piola_kirchhoff_2" => b
            .piola_kirchhoff_2
            .as_mut()
            .ok_or_else(|| Error::invalid("piola_kirchhoff_2 is not registered")),
        _ => Err(Error::not_found("internal", name)),
    }
}

impl DataAccessor<Element> for StressAccessor<'_> {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        let Ok(split) = split_by_law(self.materials, elements) else {
            return usize::MAX;
        };
        let mut size = 0usize;
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let Ok(l) = self.materials.law(law) else {
                return usize::MAX;
            };
            let per_q = Self::fields(l, tag).len() * self.dim * self.dim * R;
            for run in type_runs(els, |x| x.0) {
                let Ok(nq) = self.nq(run[0].0.ty()) else {
                    return usize::MAX;
                };
                size = size.saturating_add(per_q.saturating_mul(nq).saturating_mul(run.len()));
            }
        }
        size
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        let split = split_by_law(self.materials, elements)?;
        let dd = self.dim * self.dim;
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let l = self.materials.law(law)?;
            for name in Self::fields(l, tag) {
                let f = field(l, name)?;
                for run in type_runs(els, |x| x.0) {
                    let (ty, ghost) = (run[0].0.ty(), run[0].0.ghost_type());
                    let nq = self.nq(ty)?;
                    let a = f.get(ty, ghost)?;
                    let a = a.as_slice();
                    let w = nq * dd;
                    for &(e, local) in run {
                        let start = local * w;
                        let vals = a.get(start..start + w).ok_or_else(|| {
                            Error::invalid(format!("{e}: {name} has no values (law-local {local})"))
                        })?;
                        buffer.pack_slice(vals);
                    }
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
        let split = split_by_law(self.materials, elements)?;
        let dd = self.dim * self.dim;
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let names = Self::fields(self.materials.law(law)?, tag);
            for name in names {
                let mut nqs = Vec::new();
                for run in type_runs(els, |x| x.0) {
                    nqs.push(self.nq(run[0].0.ty())?);
                }
                let l = self.materials.law_mut(law)?;
                let f = field_mut(l, name)?;
                for (run, &nq) in type_runs(els, |x| x.0).zip(&nqs) {
                    let (ty, ghost) = (run[0].0.ty(), run[0].0.ghost_type());
                    let mut a = f.get_mut(ty, ghost)?;
                    let a = a.as_mut_slice();
                    let w = nq * dd;
                    for &(e, local) in run {
                        let start = local * w;
                        let vals = a.get_mut(start..start + w).ok_or_else(|| {
                            Error::invalid(format!("{e}: {name} has no values (law-local {local})"))
                        })?;
                        buffer.unpack_slice(vals)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Model element tags whose data upstream packs but never unpacks
/// (`_smm_mass`: mass; `_smm_for_gradu`: displacement; `_smm_boundary`:
/// external force, velocity, blocked flags; `_for_dump`: displacement,
/// velocity, acceleration, internal force, external force), each per node
/// of the elements in connectivity order. The receiver reads and discards
/// them (UD-233, see the module docs).
pub(crate) struct ElementNodalAccessor<'a> {
    pub mesh: &'a Mesh,
    pub dim: usize,
    /// Arrays packed for the tag, in upstream order (`bool` arrays are
    /// packed as one byte per value).
    pub arrays: Vec<NodalArray<'a>>,
}

/// A nodal array packed by [`ElementNodalAccessor`] / [`NodalAccessor`].
#[derive(Clone, Copy)]
pub(crate) enum NodalArray<'a> {
    /// `dim` reals per node.
    Real(&'a Array<Real>),
    /// `dim` booleans per node.
    Bool(&'a Array<bool>),
}

impl NodalArray<'_> {
    fn bytes_per_value(self) -> usize {
        match self {
            NodalArray::Real(_) => R,
            NodalArray::Bool(_) => 1,
        }
    }
    fn pack(self, buffer: &mut CommunicationBuffer, node: Idx) -> Result<()> {
        let n = akantu_core::try_to_usize(node)?;
        match self {
            NodalArray::Real(a) => buffer.pack_slice(a.get_tuple(n)?),
            NodalArray::Bool(a) => buffer.pack_slice(a.get_tuple(n)?),
        }
        Ok(())
    }
}

impl ElementNodalAccessor<'_> {
    fn nb_nodes(&self, elements: &[Element]) -> usize {
        elements
            .iter()
            .map(|e| e.ty().nb_nodes_per_element())
            .fold(0usize, usize::saturating_add)
    }
}

impl DataAccessor<Element> for ElementNodalAccessor<'_> {
    fn nb_data(&self, elements: &[Element], _tag: SynchronizationTag) -> usize {
        let per_node: usize = self
            .arrays
            .iter()
            .map(|a| a.bytes_per_value() * self.dim)
            .sum();
        self.nb_nodes(elements).saturating_mul(per_node)
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        _tag: SynchronizationTag,
    ) -> Result<()> {
        for a in &self.arrays {
            for &e in elements {
                for &n in &self.mesh.element_connectivity(e)? {
                    a.pack(buffer, n)?;
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
        // upstream: never unpacked; consume and discard
        let n = self.nb_data(elements, tag);
        let mut sink = vec![0u8; n];
        buffer.unpack_slice(&mut sink)
    }
}

/// Model DOF tags (upstream `getNbData/packData/unpackData(Array<Idx>)`):
/// `dim` values per node of each array, packed and unpacked (`_smm_uv`:
/// displacement, velocity; `_smm_res`: internal force; `_smm_mass`: mass;
/// `_for_dump`: displacement, velocity, acceleration, internal force,
/// external force).
pub(crate) struct NodalAccessor<'a> {
    pub dim: usize,
    pub arrays: Vec<&'a mut Array<Real>>,
}

impl DataAccessor<Idx> for NodalAccessor<'_> {
    fn nb_data(&self, nodes: &[Idx], _tag: SynchronizationTag) -> usize {
        nodes
            .len()
            .saturating_mul(self.dim * R)
            .saturating_mul(self.arrays.len())
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        nodes: &[Idx],
        _tag: SynchronizationTag,
    ) -> Result<()> {
        for a in &self.arrays {
            for &n in nodes {
                buffer.pack_slice(a.get_tuple(akantu_core::try_to_usize(n)?)?);
            }
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        nodes: &[Idx],
        _tag: SynchronizationTag,
    ) -> Result<()> {
        for a in &mut self.arrays {
            for &n in nodes {
                buffer.unpack_slice(a.get_tuple_mut(akantu_core::try_to_usize(n)?)?)?;
            }
        }
        Ok(())
    }
}

/// Law-specific data of a synchronisation tag (upstream `Material`
/// overrides of `getNbData` / `packData` / `unpackData`, reached through
/// `ConstitutiveLawsHandler::splitByConstitutiveLaw`): per law (law order,
/// then element order), the `Real` internals named by
/// [`Material::exchanged_internals`], each packed element by element (all
/// quadrature points and components of the element, upstream
/// `packInternalFieldHelper`). Marigo `_clh_init_cl` (`Yd`,
/// MPI-041), phase field `_smm_density` (`damage`, `grad_u`, MPI-036), user
/// laws' extension tags (MPI-053).
pub(crate) struct LawDataAccessor<'a> {
    pub materials: &'a mut ConstitutiveLawsHandler<dyn Material>,
    /// Quadrature points per element type (the model's FE engine).
    pub nb_quad: &'a ElementTypeMap<usize>,
}

impl LawDataAccessor<'_> {
    fn nq(&self, ty: ElementType) -> Result<usize> {
        self.nb_quad
            .get(ty, GhostType::NotGhost)
            .copied()
            .ok_or_else(|| Error::not_found("integration points of", ty.name()))
    }

    fn nb_data_checked(&self, elements: &[Element], tag: SynchronizationTag) -> Result<usize> {
        let split = split_by_law(self.materials, elements)?;
        let mut size = 0usize;
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let l = self.materials.law(law)?;
            for name in l.exchanged_internals(tag) {
                let nbc = real_internal_components(l, name)?;
                for &(e, _) in els {
                    size = size.saturating_add(self.nq(e.ty())?.saturating_mul(nbc * R));
                }
            }
        }
        Ok(size)
    }
}

/// Components of the `Real` internal `name` of `law`.
fn real_internal_components(law: &dyn Material, name: &str) -> Result<usize> {
    let mut found: Option<Result<usize>> = None;
    law.for_each_internal(&mut |n, f| {
        if n == name && found.is_none() {
            found = Some(match f {
                akantu_model::constitutive_law::InternalFieldRef::Real(f) => Ok(f.nb_component()),
                _ => Err(Error::invalid(format!(
                    "{}: internal {name} exchanged for a synchronisation tag is not a Real field",
                    law.law_id()
                ))),
            });
        }
    });
    found.unwrap_or_else(|| {
        Err(Error::not_found(
            "internal field",
            format!("{name} of {}", law.law_id()),
        ))
    })
}

impl DataAccessor<Element> for LawDataAccessor<'_> {
    /// A local error (an element without law, an unknown internal) is
    /// reported by `pack_data` / `unpack_data`, which recompute the size;
    /// the synchronizer's preflight agrees it on every rank.
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        self.nb_data_checked(elements, tag).unwrap_or(usize::MAX)
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        let split = split_by_law(self.materials, elements)?;
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let l = self.materials.law(law)?;
            for name in l.exchanged_internals(tag) {
                let nbc = real_internal_components(l, name)?;
                let mut nqs = Vec::with_capacity(els.len());
                for &(e, _) in els {
                    nqs.push(self.nq(e.ty())?);
                }
                let mut r: Result<()> = Ok(());
                l.for_each_internal(&mut |n, f| {
                    let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f else {
                        return;
                    };
                    if n != name || r.is_err() {
                        return;
                    }
                    r = (|| {
                        for (&(e, local), &nq) in els.iter().zip(&nqs) {
                            let a = f.get(e.ty(), e.ghost_type())?;
                            let start = local * nq * nbc;
                            let vals =
                                a.as_slice().get(start..start + nq * nbc).ok_or_else(|| {
                                    Error::invalid(format!(
                                        "{e}: {name} has no values (law-local {local})"
                                    ))
                                })?;
                            buffer.pack_slice(vals);
                        }
                        Ok(())
                    })();
                });
                r?;
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
        let split = split_by_law(self.materials, elements)?;
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let names: Vec<String> = self
                .materials
                .law(law)?
                .exchanged_internals(tag)
                .into_iter()
                .map(str::to_owned)
                .collect();
            let mut nqs = Vec::with_capacity(els.len());
            for &(e, _) in els {
                nqs.push(self.nq(e.ty())?);
            }
            for name in &names {
                let name = name.as_str();
                let l = self.materials.law_mut(law)?;
                let nbc = real_internal_components(l, name)?;
                let mut done = false;
                l.for_each_internal_mut(&mut |n, f| {
                    let akantu_model::constitutive_law::InternalFieldMut::Real(f) = f else {
                        return Ok(());
                    };
                    if n != name || done {
                        return Ok(());
                    }
                    done = true;
                    for (&(e, local), &nq) in els.iter().zip(&nqs) {
                        let mut a = f.get_mut(e.ty(), e.ghost_type())?;
                        let start = local * nq * nbc;
                        let vals = a
                            .as_mut_slice()
                            .get_mut(start..start + nq * nbc)
                            .ok_or_else(|| {
                                Error::invalid(format!(
                                    "{e}: {name} has no values (law-local {local})"
                                ))
                            })?;
                        buffer.unpack_slice(vals)?;
                    }
                    Ok(())
                })?;
            }
        }
        Ok(())
    }
}

/// The generic exchanges of `tag` (`_smm_stress`, `_smm_gradu`,
/// `_smm_mass`, `_for_dump`, `_smm_boundary`) carry no law-specific data:
/// a law naming internals for one of them is refused (no built-in law does,
/// upstream's built-in laws add none either) instead of silently not
/// exchanging them.
pub(crate) fn check_no_law_data(
    materials: &ConstitutiveLawsHandler<dyn Material>,
    tag: SynchronizationTag,
) -> Result<()> {
    if let Some(l) = materials
        .laws()
        .iter()
        .find(|l| !l.exchanged_internals(tag).is_empty())
    {
        return Err(Error::Unsupported(format!(
            "{}: law-specific data for the model tag {tag} (only _smm_density, _clh_init_cl \
             and the extension tags carry law data)",
            l.law_id()
        )));
    }
    Ok(())
}

/// Law data are exchanged only with `_smm_density`,
/// `_clh_init_cl` and the extension tags `_test`, `_user_1`, `_user_2`; a
/// law declaring internals ([`crate::Material::exchanged_internals`]) for
/// any other tag would see them silently dropped (the residual's
/// `_smm_stress` exchange, the generic accessors and the tags the model
/// does not handle carry no law data). Checked once for every law at
/// `init_materials`, serial and distributed alike (on a distributed mesh
/// inside the agreed selection step).
///
/// `_mnl_for_average` and `_mnl_weight`
/// stay refused here on purpose: they have their own path, the non-local
/// manager's accessor over its flattened per-point arrays
/// (`akantu_model::non_local::ghosts`, upstream's neighbourhood and
/// weight-function pack / unpack): the averaged variables are what the
/// laws declare as `MaterialNonLocal::non_local_variables`, the weight data
/// what the weight function declares as its internals, so a law never
/// declares law data for those tags.
pub(crate) fn check_law_data_tags(materials: &ConstitutiveLawsHandler<dyn Material>) -> Result<()> {
    use SynchronizationTag as T;
    for &tag in SynchronizationTag::ALL {
        if matches!(
            tag,
            T::SmmDensity | T::ClhInitCl | T::Test | T::User1 | T::User2
        ) {
            continue;
        }
        check_no_law_data(materials, tag)?;
    }
    Ok(())
}

/// Laws whose parallel behaviour needs law-specific exchanges not yet
/// ported: refused on distributed meshes instead of silently computing
/// wrong ghosts. Marigo (`_clh_init_cl`, MPI-041) and the phase-field law
/// (`_smm_density`, MPI-036) exchange their data through
/// [`LawDataAccessor`]; the non-local laws average over the
/// grid synchronisers (MPI-048…050). No law is refused
/// today; the check stays the place for a future one.
pub(crate) fn check_parallel_support(
    _materials: &ConstitutiveLawsHandler<dyn Material>,
) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodal_accessor_round_trips_in_array_order() {
        let mut u = Array::from_vec(vec![1.0, 2.0, 3.0, 4.0], 2).unwrap();
        let mut v = Array::from_vec(vec![5.0, 6.0, 7.0, 8.0], 2).unwrap();
        let mut b = CommunicationBuffer::new();
        {
            let acc = NodalAccessor {
                dim: 2,
                arrays: vec![&mut u, &mut v],
            };
            assert_eq!(acc.nb_data(&[1], SynchronizationTag::SmmUv), 32);
            acc.pack_data(&mut b, &[1], SynchronizationTag::SmmUv)
                .unwrap();
        }
        let mut u2 = Array::from_vec(vec![0.0; 4], 2).unwrap();
        let mut v2 = Array::from_vec(vec![0.0; 4], 2).unwrap();
        let mut acc = NodalAccessor {
            dim: 2,
            arrays: vec![&mut u2, &mut v2],
        };
        b.rewind();
        acc.unpack_data(&mut b, &[0], SynchronizationTag::SmmUv)
            .unwrap();
        assert_eq!(u2.as_slice(), &[3.0, 4.0, 0.0, 0.0]);
        assert_eq!(v2.as_slice(), &[7.0, 8.0, 0.0, 0.0]);
    }
}
