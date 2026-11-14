// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/constitutive_laws/
// {constitutive_laws_handler.hh, constitutive_laws_handler_tmpl.hh}
// (law registry, instantiation from parser sections, element assignment).

//! The law registry of a model (upstream `ConstitutiveLawsHandler<Law,
//! Model>`, the mixin of `SolidMechanicsModel` and the other law-based
//! models): laws in creation order, the name → index map, the per-element
//! law index and law-local numbering, and the selector chain.
//!
//! The model owns one handler and drives it:
//! [`ConstitutiveLawsHandler::instantiate_laws`] on the model's parser
//! section (or the root section when the file has no model section),
//! [`ConstitutiveLawsHandler::assign_laws_to_elements`], then each law's
//! initialisation (`initMaterial`). Ghost elements are assigned by
//! synchronisation in upstream (tag `_constitutive_law_id`); this
//! handler assigns not-ghost elements only, as upstream's
//! `assignConstitutiveLawToElements`.

use std::collections::BTreeMap;

use akantu_core::{
    Element, ElementType, ElementTypeMapArray, Error, GhostType, Id, Idx, Result,
    SharedElementTypeMapArray, TypeFilter,
};
use akantu_mesh::Mesh;
use akantu_parser::{ParserSection, ParserType};

use crate::constitutive_law::{
    ConstitutiveLaw, ConstitutiveLawFactory, LawSpec, ModelServices, SelectorChain, SelectorContext,
};

/// Outcome of [`ConstitutiveLawsHandler::select_laws`]: the selected
/// elements in selection order and their law indices, to be committed with
/// [`ConstitutiveLawsHandler::commit_selection`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[must_use]
pub struct LawSelection {
    elements: Vec<Element>,
    chosen: Vec<usize>,
}

impl LawSelection {
    /// Number of selected elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// `true` if no element was selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

/// Law registry of a model (upstream `ConstitutiveLawsHandler`).
#[derive(Debug)]
pub struct ConstitutiveLawsHandler<L: ?Sized> {
    model_id: Id,
    spatial_dimension: usize,
    parser_type: ParserType,
    laws: Vec<Box<L>>,
    names: BTreeMap<String, usize>,
    /// Exportable (D-047): Python `getMaterialByElement()` views alias it.
    law_index: SharedElementTypeMapArray<Idx>,
    local_numbering: ElementTypeMapArray<Idx>,
    /// Element → law selection (upstream `constitutive_law_selector`,
    /// default `DefaultConstitutiveLawSelector(constitutive_law_index)`
    /// with fallback 0).
    pub selector: SelectorChain,
}

impl<L: ?Sized + ConstitutiveLaw> ConstitutiveLawsHandler<L> {
    /// New handler for the model `model_id` on `mesh` (upstream
    /// constructor: `constitutive_law_index` initialised to `-1` and
    /// `constitutive_law_local_numbering` to 0 for every element type **of
    /// the mesh's spatial dimension** (the default `_spatial_dimension` of
    /// `ElementTypeMapArray::initialize(mesh, …)`), all kinds, both ghost
    /// types; lower-dimensional (boundary) elements get no entry).
    pub fn new(
        model_id: impl Into<Id>,
        mesh: &Mesh,
        spatial_dimension: usize,
        parser_type: ParserType,
    ) -> Result<Self> {
        let model_id = model_id.into();
        let mut law_index =
            SharedElementTypeMapArray::new(format!("{model_id}:constitutive_law index"));
        let mut local_numbering =
            ElementTypeMapArray::new(format!("{model_id}:constitutive_law local numbering"));
        for ghost in akantu_core::GhostType::ALL {
            let filter = TypeFilter {
                spatial_dimension: Some(mesh.spatial_dimension()),
                ghost_type: ghost,
                kind: None,
            };
            for ty in mesh.element_types(filter) {
                let n = mesh.nb_element(ty, ghost);
                law_index.alloc(n, 1, ty, ghost, -1)?;
                local_numbering.alloc(n, 1, ty, ghost, 0)?;
            }
        }
        Ok(Self {
            model_id,
            spatial_dimension,
            parser_type,
            laws: Vec::new(),
            names: BTreeMap::new(),
            law_index,
            local_numbering,
            selector: SelectorChain::upstream_default(),
        })
    }

    /// Model id (prefix of the law ids).
    #[must_use]
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Parser section type of the laws (`material` for solids).
    #[must_use]
    pub fn parser_type(&self) -> ParserType {
        self.parser_type
    }

    /// Register a law of type `law_type` named `name` (upstream
    /// `registerNewConstitutiveLaw(name, type, option)`): id
    /// `<model id>:<index>:<type>`, allocated by `factory`. The law's `name`
    /// parameter is **not** set (upstream sets it by parsing the section).
    ///
    /// # Errors
    /// A duplicate name (upstream: debug assertion), an unknown type, or the
    /// allocator's error.
    pub fn register_new_law(
        &mut self,
        factory: &ConstitutiveLawFactory<L>,
        name: &str,
        law_type: &str,
        option: &str,
        services: &dyn ModelServices,
    ) -> Result<usize> {
        if self.names.contains_key(name) {
            return Err(Error::AlreadyExists {
                kind: "constitutive law name",
                id: name.into(),
            });
        }
        let index = self.laws.len();
        let spec = LawSpec {
            spatial_dimension: self.spatial_dimension,
            option: option.into(),
            id: format!("{}:{index}:{law_type}", self.model_id),
        };
        let law = factory.create(law_type, &spec, services)?;
        self.names.insert(name.into(), index);
        self.laws.push(law);
        Ok(index)
    }

    /// Register and parse a law from its input-file section (upstream
    /// `registerNewConstitutiveLaw(section)`): type = section name, option =
    /// section option, name = the section's `name` parameter (current
    /// scope, raw value), then `parseSection`.
    ///
    /// # Errors
    /// A section without `name` (upstream `AKANTU_ERROR`), registration or
    /// parsing errors.
    pub fn register_new_law_from_section(
        &mut self,
        factory: &ConstitutiveLawFactory<L>,
        section: &ParserSection,
        services: &dyn ModelServices,
        permissive: bool,
    ) -> Result<usize> {
        let name = section.parameter("name").ok_or_else(|| {
            Error::invalid(format!(
                "A constitutive_law of type '{}' in the input file has been defined without a name!",
                section.name
            ))
        })?;
        let name = name.value.to_string_lossy().into_owned();
        let index =
            self.register_new_law(factory, &name, &section.name, &section.option, services)?;
        self.laws[index].parse_section(section, permissive)?;
        Ok(index)
    }

    /// Upstream `instantiateConstitutiveLaws(section)`: one law per
    /// sub-section of the handler's parser type, in file order.
    pub fn instantiate_laws(
        &mut self,
        factory: &ConstitutiveLawFactory<L>,
        section: &ParserSection,
        services: &dyn ModelServices,
        permissive: bool,
    ) -> Result<()> {
        let subs: Vec<&ParserSection> = section.sub_sections_of(self.parser_type).collect();
        for s in subs {
            self.register_new_law_from_section(factory, s, services, permissive)?;
        }
        // Upstream warns with AKANTU_DEBUG_WARNING when no law was
        // instantiated: compiled out in the release-like oracle build.
        Ok(())
    }

    /// Upstream `assignConstitutiveLawToElements(filter)`: for every
    /// not-ghost element of the mesh's spatial dimension (`for_each_element`
    /// default; of `filter` if given, restricted to that dimension too; all
    /// kinds, enum order — boundary elements are never presented to the
    /// selector), choose its law with the selector chain and store it **before
    /// the next element is selected** (upstream writes
    /// `constitutive_law_index(element)` inside the selection loop, so a
    /// selector sees the decisions already taken in the same pass),
    /// then add every element to its law's filter (second
    /// pass, upstream order) and record the law-local index. Laws'
    /// internals are not resized here.
    ///
    /// # Errors
    /// A selected index without a law (upstream: debug assertion, then
    /// out-of-bounds access), or a selector error. The first pass stops at
    /// the failing element: the elements selected before it keep their new
    /// law index (as upstream's sequential writes), no element is added to
    /// a law.
    pub fn assign_laws_to_elements(
        &mut self,
        mesh: &Mesh,
        filter: Option<&ElementTypeMapArray<Idx>>,
    ) -> Result<()> {
        let selection = self.select_laws(mesh, filter)?;
        self.commit_selection(selection)
    }

    /// First pass of [`ConstitutiveLawsHandler::assign_laws_to_elements`]:
    /// choose and store the law index of every element (the selector sees
    /// the decisions already taken); no element is added to a law. On a
    /// distributed mesh the outcome of this pass is agreed before
    /// [`ConstitutiveLawsHandler::commit_selection`], so
    /// a rank-specific selector failure leaves every rank in this
    /// retryable state (law indices overwritten by the next selection,
    /// filters unchanged).
    ///
    /// # Errors
    /// As [`ConstitutiveLawsHandler::assign_laws_to_elements`]'s first
    /// pass.
    pub fn select_laws(
        &mut self,
        mesh: &Mesh,
        filter: Option<&ElementTypeMapArray<Idx>>,
    ) -> Result<LawSelection> {
        let dim = mesh.spatial_dimension();
        let elements: Vec<Element> = match filter {
            Some(f) => f
                .as_map()
                .iter(akantu_core::GhostType::NotGhost)
                .filter(|(ty, _)| ty.spatial_dimension() == dim)
                .flat_map(|(ty, a)| {
                    a.as_slice()
                        .iter()
                        .map(move |&i| Element::new(ty, i, akantu_core::GhostType::NotGhost))
                })
                .collect(),
            None => mesh
                .elements(TypeFilter {
                    spatial_dimension: Some(dim),
                    ghost_type: akantu_core::GhostType::NotGhost,
                    kind: None,
                })
                .collect(),
        };
        let mut chosen = Vec::with_capacity(elements.len());
        for &e in &elements {
            let i = {
                // A short-lived context per element: the selector sees the
                // law-index map with every earlier decision of this pass.
                let names = &self.names;
                let by_name = |n: &str| names.get(n).copied();
                let ctx = SelectorContext {
                    mesh,
                    law_index: &self.law_index,
                    law_by_name: &by_name,
                };
                self.selector.select(&ctx, e)?
            };
            if i >= self.laws.len() {
                return Err(Error::OutOfRange {
                    what: "constitutive law (selector result)",
                    index: i,
                    len: self.laws.len(),
                });
            }
            self.law_index.set_element(e, &[akantu_core::try_idx(i)?])?;
            chosen.push(i);
        }
        Ok(LawSelection { elements, chosen })
    }

    /// Second pass of [`ConstitutiveLawsHandler::assign_laws_to_elements`]:
    /// add every selected element to its law's filter (selection order)
    /// and record the law-local index.
    ///
    /// # Errors
    /// A law index out of range (a selection taken before laws were
    /// removed) or a law refusing the element.
    pub fn commit_selection(&mut self, selection: LawSelection) -> Result<()> {
        let LawSelection { elements, chosen } = selection;
        for (&e, &i) in elements.iter().zip(&chosen) {
            let len = self.laws.len();
            let law = self.laws.get_mut(i).ok_or(Error::OutOfRange {
                what: "constitutive law (selection)",
                index: i,
                len,
            })?;
            let local = law.add_element(e)?;
            self.local_numbering.element_mut(e)?[0] = local;
        }
        Ok(())
    }

    /// Number of laws.
    #[must_use]
    pub fn nb_laws(&self) -> usize {
        self.laws.len()
    }

    /// Laws in creation order.
    #[must_use]
    pub fn laws(&self) -> &[Box<L>] {
        &self.laws
    }

    /// Laws, mutable.
    pub fn laws_mut(&mut self) -> &mut [Box<L>] {
        &mut self.laws
    }

    /// Law by index.
    pub fn law(&self, index: usize) -> Result<&L> {
        self.laws
            .get(index)
            .map(AsRef::as_ref)
            .ok_or(Error::OutOfRange {
                what: "constitutive law",
                index,
                len: self.laws.len(),
            })
    }

    /// Law by index, mutable.
    pub fn law_mut(&mut self, index: usize) -> Result<&mut L> {
        let len = self.laws.len();
        self.laws
            .get_mut(index)
            .map(AsMut::as_mut)
            .ok_or(Error::OutOfRange {
                what: "constitutive law",
                index,
                len,
            })
    }

    /// Index of the law named `name` (upstream `getConstitutiveLawIndex`).
    pub fn law_index_of(&self, name: &str) -> Result<usize> {
        self.names.get(name).copied().ok_or_else(|| {
            Error::not_found(
                "constitutive law",
                format!(
                    "{name} (the model {} has no constitutive_law named {name})",
                    self.model_id
                ),
            )
        })
    }

    /// Law of `element` (upstream `getConstitutiveLaw(element)`).
    pub fn law_of(&self, element: Element) -> Result<&L> {
        let i = self.law_index.element_ref(element)?[0];
        let i = akantu_core::decode_idx(i)?
            .ok_or_else(|| Error::invalid(format!("{element} has no constitutive law")))?;
        self.law(i)
    }

    /// Per-element law index (`-1` = none).
    #[must_use]
    pub fn law_index_map(&self) -> &SharedElementTypeMapArray<Idx> {
        &self.law_index
    }

    /// Per-element law index, mutable (upstream: users fill it before
    /// `initFull` to drive the default selector).
    pub fn law_index_map_mut(&mut self) -> &mut SharedElementTypeMapArray<Idx> {
        &mut self.law_index
    }

    /// Per-element law-local numbering.
    #[must_use]
    pub fn local_numbering_map(&self) -> &ElementTypeMapArray<Idx> {
        &self.local_numbering
    }

    /// Per-element law-local numbering, mutable (grown on
    /// `ElementsAdded` and compacted on `ElementsRemoved` together with
    /// [`ConstitutiveLawsHandler::law_index_map_mut`]).
    pub fn local_numbering_map_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        &mut self.local_numbering
    }

    /// Record the handler entries returned by
    /// [`ConstitutiveLaw::on_elements_removed`] of law `law` (upstream
    /// writes them inside `onElementsRemoved`).
    pub fn record_renumbered(&mut self, law: usize, entries: &[(Element, Idx)]) -> Result<()> {
        let li = akantu_core::try_idx(law)?;
        for &(e, local) in entries {
            self.law_index.set_element(e, &[li])?;
            self.local_numbering.element_mut(e)?[0] = local;
        }
        Ok(())
    }

    /// Upstream `reassignConstitutiveLaw()` (`constitutive_laws_handler_tmpl.hh:
    /// 337-362`): ask the selector again for every element of the mesh's
    /// spatial dimension (both ghost types, `for_each_element`); for every
    /// element whose law changes, then per law in creation order:
    /// `removeElements` (the law's filter compacted, its internals
    /// renumbered, `removeIntegrationPoints`) and `addElements` (appended
    /// in mesh order, internals resized). The handler's law index and
    /// law-local numbering follow (upstream records them in the laws'
    /// `onElementsRemoved` / `addElements`). A model with laws whose
    /// internals need more than [`ConstitutiveLaw`]'s generic resize keeps
    /// its own reassignment (the solid model's `reassign_material`).
    ///
    /// # Errors
    /// An element without a law (upstream indexes `element_to_remove[-1]`),
    /// a selector result out of range, errors of the laws.
    pub fn reassign_constitutive_law(
        &mut self,
        mesh: &Mesh,
        nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
    ) -> Result<()> {
        let dim = mesh.spatial_dimension();
        let elements: Vec<Element> = GhostType::ALL
            .into_iter()
            .flat_map(|ghost| {
                mesh.elements(TypeFilter {
                    spatial_dimension: Some(dim),
                    ghost_type: ghost,
                    kind: None,
                })
            })
            .collect();
        let nb = self.laws.len();
        let mut to_add: Vec<Vec<Element>> = vec![Vec::new(); nb];
        let mut to_remove: Vec<Vec<Element>> = vec![Vec::new(); nb];
        for e in elements {
            let old = self.law_index.element_ref(e)?[0];
            let new = {
                let names = &self.names;
                let by_name = |n: &str| names.get(n).copied();
                let ctx = SelectorContext {
                    mesh,
                    law_index: &self.law_index,
                    law_by_name: &by_name,
                };
                self.selector.select(&ctx, e)?
            };
            if new >= nb {
                return Err(Error::OutOfRange {
                    what: "constitutive law (selector result)",
                    index: new,
                    len: nb,
                });
            }
            match akantu_core::decode_idx(old)? {
                Some(o) if o != new => {
                    to_add[new].push(e);
                    to_remove[o].push(e);
                }
                Some(_) => {}
                None => {
                    return Err(Error::invalid(format!(
                        "reassignConstitutiveLaw: {e} has no constitutive law (upstream indexes \
                         element_to_remove[-1])"
                    )));
                }
            }
        }
        for i in 0..nb {
            if !to_remove[i].is_empty() {
                let kept = remove_law_elements(&mut *self.laws[i], &to_remove[i], nb_quad)?;
                self.record_renumbered(i, &kept)?;
            }
            if !to_add[i].is_empty() {
                let local = self.laws[i].add_elements(&to_add[i], nb_quad)?;
                let entries: Vec<(Element, Idx)> = to_add[i].iter().copied().zip(local).collect();
                self.record_renumbered(i, &entries)?;
            }
        }
        Ok(())
    }
}

/// Upstream `ConstitutiveLaw::removeElements(elements)`: the filter without
/// `elements` (order kept), the renumbering old → new local index (`-1`
/// for a removed element), `removeIntegrationPoints`; returns the handler
/// entries `(element, new local index)` of the kept elements.
fn remove_law_elements<L: ?Sized + ConstitutiveLaw>(
    law: &mut L,
    elements: &[Element],
    nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
) -> Result<Vec<(Element, Idx)>> {
    let leaving: std::collections::HashSet<Element> = elements.iter().copied().collect();
    let mut kept = Vec::new();
    let mut renumbering = ElementTypeMapArray::<Idx>::new("remove constitutive law filter elem");
    let mut new_filters = Vec::new();
    let blocks: Vec<(ElementType, GhostType)> = law
        .element_filter()
        .as_map()
        .iter_all()
        .map(|(t, g, _)| (t, g))
        .collect();
    for (ty, ghost) in blocks {
        let filter = law.element_filter().get(ty, ghost)?.as_slice().to_vec();
        let mut num = Vec::with_capacity(filter.len());
        let mut new_filter = Vec::with_capacity(filter.len());
        for &g in &filter {
            let el = Element::new(ty, g, ghost);
            if leaving.contains(&el) {
                num.push(-1);
                continue;
            }
            let ni = akantu_core::try_idx(new_filter.len())?;
            num.push(ni);
            new_filter.push(g);
            kept.push((el, ni));
        }
        renumbering
            .as_map_mut()
            .insert(ty, ghost, akantu_core::Array::from_vec(num, 1)?)?;
        new_filters.push((ty, ghost, new_filter));
    }
    for (ty, ghost, f) in new_filters {
        *law.element_filter_mut().get_mut(ty, ghost)? = akantu_core::Array::from_vec(f, 1)?;
    }
    law.remove_integration_points(&renumbering, nb_quad)?;
    Ok(kept)
}

/// `_constitutive_law_id` accessor of a law handler (upstream
/// `ConstitutiveLawsHandler::getNbData` / `packData` / `unpackData` for that
/// tag, `constitutive_laws_handler_tmpl.hh:472-531`): owners send the law
/// index of their elements; a ghost element without a law takes the
/// received one and is appended to that law's ghost filter **in unpack
/// order** (receive schemes in rank order, then scheme order; upstream
/// unpacks in `MPI_Waitany` completion order, UD-232). Generic over the
/// law type (the phase-field model; the solid and diffusion
/// models keep their own copies of the same protocol).
#[derive(Debug)]
pub struct LawIdAccessor<'a, L: ?Sized> {
    /// The handler whose ghost elements receive their law.
    pub laws: &'a mut ConstitutiveLawsHandler<L>,
}

impl<L: ?Sized + ConstitutiveLaw> akantu_comm::DataAccessor<Element> for LawIdAccessor<'_, L> {
    fn nb_data(&self, elements: &[Element], tag: akantu_core::enums::SynchronizationTag) -> usize {
        if tag == akantu_core::enums::SynchronizationTag::ConstitutiveLawId {
            elements.len().saturating_mul(std::mem::size_of::<Idx>())
        } else {
            0
        }
    }
    fn pack_data(
        &self,
        buffer: &mut akantu_comm::buffer::CommunicationBuffer,
        elements: &[Element],
        tag: akantu_core::enums::SynchronizationTag,
    ) -> Result<()> {
        if tag != akantu_core::enums::SynchronizationTag::ConstitutiveLawId {
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
        buffer: &mut akantu_comm::buffer::CommunicationBuffer,
        elements: &[Element],
        tag: akantu_core::enums::SynchronizationTag,
    ) -> Result<()> {
        if tag != akantu_core::enums::SynchronizationTag::ConstitutiveLawId {
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
///
/// # Errors
/// An element without law, or a law index out of range.
pub fn split_by_law<L: ?Sized + ConstitutiveLaw>(
    laws: &ConstitutiveLawsHandler<L>,
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
