// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/constitutive_laws/
// {constitutive_law.hh, constitutive_law_tmpl.hh, constitutive_laws_handler.hh,
// constitutive_law_selector.hh, constitutive_law_selector_tmpl.hh},
// src/common/aka_factory.hh.

//! Generic constitutive-law framework (decision record D-016).
//!
//! * Each law is a struct holding its parameters and **typed**
//!   [`InternalField`]s as plain members (no string-keyed map in hot loops).
//! * [`ConstitutiveLaw`] is the object-safe, model-independent part
//!   (parameters, internal-field reflection, history, element filter);
//!   model-specific behaviour is a sub-trait in the model crate
//!   (`akantu_solid::Material`, diffusion laws, phase-field laws), whose
//!   batch methods are called once per `(type, ghost)` so that the virtual
//!   call is outside the quadrature-point loop.
//! * Laws are created by name through a [`ConstitutiveLawFactory`] (upstream
//!   `Factory<...>` singletons with static registration). Built-ins are
//!   registered explicitly by each model crate's `register_builtins`
//!   (no life-before-main); users and Python can register more at runtime.

use std::collections::BTreeMap;
use std::sync::Arc;

use akantu_core::array::{ArrayView, ArrayViewMut};
use akantu_core::array_shared::ArrayRef;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Int, Real, Result,
    SharedElementTypeMapArray, UInt,
};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_mesh::data::MeshDataValues;
use akantu_parser::Parsable;

use crate::internal_field::{InternalField, TypeRestriction};

/// Borrowed view of an internal field of any supported value type
/// (reflection for dumpers, Python `getInternal`, event handling).
#[derive(Clone, Copy, Debug)]
pub enum InternalFieldRef<'a> {
    /// `Real` field.
    Real(&'a InternalField<Real>),
    /// `Int` field.
    Int(&'a InternalField<Int>),
    /// `UInt`/`Idx` field.
    UInt(&'a InternalField<UInt>),
    /// `bool` field.
    Bool(&'a InternalField<bool>),
}

/// Mutable view of an internal field.
#[derive(Debug)]
pub enum InternalFieldMut<'a> {
    /// `Real` field.
    Real(&'a mut InternalField<Real>),
    /// `Int` field.
    Int(&'a mut InternalField<Int>),
    /// `UInt`/`Idx` field.
    UInt(&'a mut InternalField<UInt>),
    /// `bool` field.
    Bool(&'a mut InternalField<bool>),
}

impl<'a> InternalFieldRef<'a> {
    /// `true` if the field has its own FE engine or element filter
    /// ([`InternalField::has_own_support`], CL-001).
    #[must_use]
    pub fn has_own_support(&self) -> bool {
        match self {
            Self::Real(f) => f.has_own_support(),
            Self::Int(f) => f.has_own_support(),
            Self::UInt(f) => f.has_own_support(),
            Self::Bool(f) => f.has_own_support(),
        }
    }

    /// The field's FE engine (`None`: the law's default engine;
    /// [`InternalField::fe_engine_id`], CL-001).
    #[must_use]
    pub fn fe_engine_id(&self) -> Option<&'a str> {
        match *self {
            Self::Real(f) => f.fe_engine_id(),
            Self::Int(f) => f.fe_engine_id(),
            Self::UInt(f) => f.fe_engine_id(),
            Self::Bool(f) => f.fe_engine_id(),
        }
    }

    /// The field's own element filter (`None`: the law's;
    /// [`InternalField::element_filter`], CL-001).
    #[must_use]
    pub fn element_filter(&self) -> Option<&'a ElementTypeMapArray<Idx>> {
        match *self {
            Self::Real(f) => f.element_filter(),
            Self::Int(f) => f.element_filter(),
            Self::UInt(f) => f.element_filter(),
            Self::Bool(f) => f.element_filter(),
        }
    }
}

/// `true` if an internal of `law` has its own FE engine or element filter
/// (CL-001): such a law must be resized with
/// [`ConstitutiveLaw::resize_internals_with`].
#[must_use]
pub fn has_internals_on_own_support(law: &dyn ConstitutiveLaw) -> bool {
    let mut own = false;
    law.for_each_internal(&mut |_, f| own |= f.has_own_support());
    own
}

/// `(type, ghost, nb_quadrature_points)` blocks of an internal field.
pub type InternalBlocks = Vec<(ElementType, GhostType, usize)>;

/// Blocks source of [`ConstitutiveLaw::resize_internals_with`]: the blocks
/// of `filter` on the FE engine `engine` (`None`: the law's default).
pub type InternalBlocksOf<'a> =
    dyn Fn(Option<&str>, &ElementTypeMapArray<Idx>) -> Result<InternalBlocks> + 'a;

macro_rules! on_field {
    ($f:expr, $x:ident => $e:expr) => {
        match $f {
            InternalFieldMut::Real($x) => $e,
            InternalFieldMut::Int($x) => $e,
            InternalFieldMut::UInt($x) => $e,
            InternalFieldMut::Bool($x) => $e,
        }
    };
}

macro_rules! on_ref {
    ($f:expr, $x:ident => $e:expr) => {
        match $f {
            InternalFieldRef::Real($x) => $e,
            InternalFieldRef::Int($x) => $e,
            InternalFieldRef::UInt($x) => $e,
            InternalFieldRef::Bool($x) => $e,
        }
    };
}

/// The error of a resize or compaction path that knows only the law's
/// support, for a field registered on its own (CL-001).
fn own_support_error(law: &str, field: &str, what: &str) -> Error {
    Error::Unsupported(format!(
        "internal '{field}' of law '{law}' has its own FE engine or element filter \
         (registerInternal with fe_engine_id / element_filter): {what} sizes on the law's \
         filter and default engine only; resize with ConstitutiveLaw::resize_internals_with"
    ))
}

/// Refuse a resize path that knows only the law's support when **any**
/// internal of `law` has its own (CL-001), before any field is resized
/// (the check used to run per field inside the resize
/// loop, so the fields sorted before the refused one were already
/// resized).
fn refuse_own_support<L: ConstitutiveLaw + ?Sized>(law: &L, what: &str) -> Result<()> {
    let mut own = None;
    law.for_each_internal(&mut |n, f| {
        if own.is_none() && f.has_own_support() {
            own = Some(n.to_owned());
        }
    });
    match own {
        Some(n) => Err(own_support_error(law.name(), &n, what)),
        None => Ok(()),
    }
}

/// The refusal of an element removal that an internal on its own support
/// cannot follow (CL-001, UD-1083).
fn own_support_removal_error(law: &str, field: &str, why: &str) -> Error {
    Error::Unsupported(format!(
        "element removal: internal '{field}' of law '{law}' has its own FE engine or element \
         filter (registerInternal with fe_engine_id / element_filter, CL-001): {why}; \
         refused before any change (UD-1083)"
    ))
}

/// `true` if the law-local renumbering `r` (old index → new index, `-1`
/// removed) keeps every element at its index.
fn is_identity(r: &[Idx]) -> bool {
    r.iter()
        .enumerate()
        .all(|(i, &n)| akantu_core::try_idx(i).is_ok_and(|i| i == n))
}

/// Refuse, before any change, a compaction of `law`'s internals by the
/// law-local renumbering `law_renumbering` (old law-local index → new,
/// `-1` removed; upstream `removeIntegrationPoints`) that an internal on
/// its own support cannot follow (CL-001): a field with
/// its own FE engine or element filter whose block of a renumbered
/// `(type, ghost)` holds values, when that block's renumbering is not the
/// identity (an element of the law is removed).
///
/// * A field on **its own element filter** is indexed by that filter, not
///   by the law's: upstream compacts it with the law's renumbering anyway
///   (`internal_field_tmpl.hh:222-264`; a debug assertion when the sizes
///   differ, an out-of-bounds or misattributed copy without assertions:
///   D-003 A, UD-1083).
/// * A field on **another FE engine** with the law's filter: upstream
///   compacts it correctly with its engine's point count; the port does
///   not implement that compaction and refuses (a documented difference,
///   row CL-001).
///
/// An identity renumbering (the law's elements are only renumbered in the
/// mesh, none removed) leaves every field as it is and is accepted.
///
/// # Errors
/// [`Error::Unsupported`] naming the field.
pub fn check_own_support_compaction<L: ConstitutiveLaw + ?Sized>(
    law: &L,
    law_renumbering: &ElementTypeMapArray<Idx>,
) -> Result<()> {
    let mut refused: Option<(String, &'static str)> = None;
    law.for_each_internal(&mut |n, f| {
        if refused.is_some() || !f.has_own_support() {
            return;
        }
        on_ref!(f, x => {
            for (ty, g, r) in law_renumbering.as_map().iter_all() {
                let filled = x
                    .values()
                    .as_map()
                    .get(ty, g)
                    .is_some_and(|a| !a.is_empty());
                if filled && !is_identity(r.as_slice()) {
                    let why = if x.element_filter().is_some() {
                        "its values follow its own element filter, which the law's renumbering \
                         does not describe (upstream compacts them with it anyway)"
                    } else {
                        "compacting it needs its FE engine's point count, which the port does \
                         not implement for a removal (upstream does)"
                    };
                    refused = Some((n.to_owned(), why));
                    return;
                }
            }
        });
    });
    match refused {
        Some((n, why)) => Err(own_support_removal_error(law.name(), &n, why)),
        None => Ok(()),
    }
}

/// Refuse, before any change, an element removal that renumbers or
/// removes an element of an internal's **own** element filter (CL-001):
/// the filter holds mesh element indices, which would
/// go stale (upstream keeps a reference to the caller's array and never
/// renumbers it; UD-1083).
fn check_own_filters_unchanged<L: ConstitutiveLaw + ?Sized>(
    law: &L,
    new_numbering: &ElementTypeMapArray<Idx>,
) -> Result<()> {
    let mut refused = None;
    law.for_each_internal(&mut |n, f| {
        if refused.is_some() {
            return;
        }
        let Some(own) = on_ref!(f, x => x.element_filter()) else {
            return;
        };
        for (ty, g, filter) in own.as_map().iter_all() {
            let Some(r) = new_numbering.as_map().get(ty, g) else {
                continue;
            };
            let moved = filter.as_slice().iter().any(|&e| {
                akantu_core::decode_idx(e)
                    .ok()
                    .flatten()
                    .and_then(|i| r.as_slice().get(i))
                    != Some(&e)
            });
            if moved {
                refused = Some(n.to_owned());
                return;
            }
        }
    });
    match refused {
        Some(n) => Err(own_support_removal_error(
            law.name(),
            &n,
            "the removal renumbers or removes an element of its own element filter, whose mesh \
             indices would go stale (upstream never renumbers that filter)",
        )),
        None => Ok(()),
    }
}

/// Pure part of [`ConstitutiveLaw::on_elements_removed`]: the law-local
/// renumbering, the new filters and the `(new mesh element, new law-local
/// index)` of every kept element.
struct LawRemovalPlan {
    law_renumbering: ElementTypeMapArray<Idx>,
    new_filters: Vec<(ElementType, GhostType, Vec<Idx>)>,
    kept_entries: Vec<(Element, Idx)>,
}

fn plan_law_removal<L: ConstitutiveLaw + ?Sized>(
    law: &L,
    removed: &[Element],
    new_numbering: &ElementTypeMapArray<Idx>,
) -> Result<LawRemovalPlan> {
    let mut law_renumbering =
        ElementTypeMapArray::<Idx>::new("remove constitutive law filter elem");
    let mut new_filters = Vec::new();
    let mut kept_entries = Vec::new();
    for (ty, g, renumbering) in new_numbering.as_map().iter_all() {
        let Some(filter) = law.element_filter().as_map().get(ty, g) else {
            continue;
        };
        if filter.is_empty() {
            continue;
        }
        let mut law_num = Vec::with_capacity(filter.len());
        let mut new_filter = Vec::with_capacity(filter.len());
        for &el_id in filter.as_slice() {
            let el = Element::new(ty, el_id, g);
            if removed.contains(&el) {
                law_num.push(-1);
                continue;
            }
            let old = akantu_core::try_to_usize(el_id)?;
            let new_el = *renumbering.as_slice().get(old).ok_or(Error::InvalidIndex {
                what: "mesh element renumbering",
                value: i64::from(el_id),
            })?;
            if new_el < 0 {
                return Err(Error::invalid(
                    "a not removed element has been badly renumbered",
                ));
            }
            let ni = akantu_core::try_idx(new_filter.len())?;
            law_num.push(ni);
            new_filter.push(new_el);
            kept_entries.push((Element::new(ty, new_el, g), ni));
        }
        law_renumbering
            .as_map_mut()
            .insert(ty, g, Array::from_vec(law_num, 1)?)?;
        new_filters.push((ty, g, new_filter));
    }
    Ok(LawRemovalPlan {
        law_renumbering,
        new_filters,
        kept_entries,
    })
}

/// The capability check of an element removal for `law` (CL-001):
/// refuses, **before any change**, the removal of `removed`
/// with the mesh renumbering `new_numbering` (the payload of a
/// `MeshEvent::ElementsRemoved`) when an internal on its own support could
/// not follow it ([`check_own_support_compaction`] on the law-local
/// renumbering the removal implies; an own element filter whose elements
/// the removal renumbers or removes). The models call it in their
/// pre-commit check (`MeshEventHandler::check_mesh_event`), so the
/// mutation is rolled back recoverably;
/// [`ConstitutiveLaw::on_elements_removed`] calls it again before it
/// changes the filter. A law without such internals is accepted at once.
///
/// # Errors
/// [`Error::Unsupported`] naming the field; the errors of the law-local
/// renumbering (an element outside `new_numbering`, a kept element
/// renumbered to `-1`).
pub fn check_elements_removed<L: ConstitutiveLaw + ?Sized>(
    law: &L,
    removed: &[Element],
    new_numbering: &ElementTypeMapArray<Idx>,
) -> Result<()> {
    let mut own = false;
    law.for_each_internal(&mut |_, f| own |= f.has_own_support());
    if !own {
        return Ok(());
    }
    let plan = plan_law_removal(law, removed, new_numbering)?;
    check_own_support_compaction(law, &plan.law_renumbering)?;
    check_own_filters_unchanged(law, new_numbering)
}

/// Model-independent part of every constitutive law (upstream
/// `ConstitutiveLaw<Handler>`: a `Parsable` with internal fields and an
/// element filter).
///
/// Every law is [`Parsable`] (upstream `ConstitutiveLaw` derives from
/// `Parsable(handler.getConstitutiveLawParserType(), id)`): its `.dat`
/// section is read with [`Parsable::parse_section`], which ends with
/// [`akantu_parser::ParameterRegistry::update_internal_parameters`].
pub trait ConstitutiveLaw: Parsable + std::fmt::Debug + Send {
    /// User-given name (`name = steel`).
    fn name(&self) -> &str;

    /// Law id (`<model id>:<law index>:<law type>`, upstream `getID()`).
    fn law_id(&self) -> &str;

    /// Registered type name (`elastic`, `damage_linear`, …).
    fn law_type(&self) -> &str;

    /// Elements handled by this law, per `(type, ghost)` (upstream
    /// `element_filter`): law-local index → mesh element index.
    ///
    /// A plain array, not exportable storage (D-056 item 2): Python's
    /// `getElementFilter()` returns a read-only snapshot copy.
    fn element_filter(&self) -> &ElementTypeMapArray<Idx>;

    /// Mutable element filter (the handler adds and removes elements).
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx>;

    /// Visit every internal field (name, view) in registration order.
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>));

    /// Visit every internal field mutably (resize, history, unpack).
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()>;

    /// Save history of all fields that keep it (upstream
    /// `savePreviousState`).
    fn save_previous_state(&mut self) -> Result<()> {
        self.for_each_internal_mut(&mut |_, f| match f {
            InternalFieldMut::Real(f) if f.has_history() => f.save_current_values(),
            InternalFieldMut::Int(f) if f.has_history() => f.save_current_values(),
            InternalFieldMut::UInt(f) if f.has_history() => f.save_current_values(),
            InternalFieldMut::Bool(f) if f.has_history() => f.save_current_values(),
            _ => Ok(()),
        })
    }

    /// Restore history (upstream `restorePreviousState`).
    fn restore_previous_state(&mut self) -> Result<()> {
        self.for_each_internal_mut(&mut |_, f| match f {
            InternalFieldMut::Real(f) if f.has_history() => f.restore_previous_values(),
            InternalFieldMut::Int(f) if f.has_history() => f.restore_previous_values(),
            InternalFieldMut::UInt(f) if f.has_history() => f.restore_previous_values(),
            InternalFieldMut::Bool(f) if f.has_history() => f.restore_previous_values(),
            _ => Ok(()),
        })
    }

    /// All parameter names including runtime-registered ones, in
    /// registration order (reflection for Python/printing; the static
    /// `param_names` only lists table entries).
    fn parameter_names(&self) -> Vec<String> {
        self.param_names().into_iter().map(str::to_owned).collect()
    }

    /// Named energy (upstream `getEnergy(id)`); `0` if unknown.
    fn energy(&self, energy_id: &str) -> Result<Real> {
        let _ = energy_id;
        Ok(0.0)
    }

    /// Names of the internal fields in upstream `internal_vectors` order
    /// (a `std::map` keyed by the register id: byte-wise sorted).
    fn internal_names_sorted(&self) -> Vec<String> {
        let mut names = Vec::new();
        self.for_each_internal(&mut |n, _| names.push(n.to_owned()));
        names.sort();
        names
    }

    /// Element types the law's internal fields handle by default (upstream:
    /// every `InternalField` is constructed with the law's spatial
    /// dimension and `_ek_regular` — `internal_field_tmpl.hh:31-56`,
    /// `internal_field.hh:256` — and only touches the filter types of that
    /// dimension and kind; cohesive laws use `_ek_cohesive`). `None`: no
    /// law default, each field keeps its own restriction (or none).
    /// Solid laws return their `MaterialBase` restriction (row CL-002).
    fn internal_restriction(&self) -> Option<TypeRestriction> {
        None
    }

    /// Give every internal field the law's
    /// [restriction](ConstitutiveLaw::internal_restriction) as its default
    /// ([`InternalField::adopt_restriction`]); called by
    /// [`ConstitutiveLaw::resize_internals`] and
    /// [`ConstitutiveLaw::resize_internals_from_filter`] before they
    /// resize, so a field registered after construction is restricted at
    /// its first resize.
    ///
    /// # Errors
    /// Those of [`ConstitutiveLaw::for_each_internal_mut`].
    fn apply_internal_restriction(&mut self) -> Result<()> {
        let Some(r) = self.internal_restriction() else {
            return Ok(());
        };
        self.for_each_internal_mut(&mut |_, f| {
            match f {
                InternalFieldMut::Real(f) => f.adopt_restriction(r),
                InternalFieldMut::Int(f) => f.adopt_restriction(r),
                InternalFieldMut::UInt(f) => f.adopt_restriction(r),
                InternalFieldMut::Bool(f) => f.adopt_restriction(r),
            }
            Ok(())
        })
    }

    /// Resize every internal field to the blocks `(type, ghost,
    /// nb_quadrature_points)` (upstream `resizeInternals`): fields in
    /// register-id order, blocks ghost-major in enum order — the order in
    /// which random fields draw (D-006). Each field takes the law's
    /// [restriction](ConstitutiveLaw::internal_restriction) first and then
    /// skips the blocks of types it does not handle (no allocation, no
    /// draw), as upstream.
    ///
    /// **Two resize entry points.** This method knows
    /// only the law's support and refuses a law with an internal on its own
    /// FE engine or element filter (CL-001); such laws are resized with
    /// [`ConstitutiveLaw::resize_internals_with`], which does **not** call
    /// this method. The models route a law through `resize_internals_with`
    /// only when it has such an internal
    /// ([`has_internals_on_own_support`]), so a law that overrides this
    /// method (the IGFEM laws count the points of their sub-elements here,
    /// the cohesive laws size on their own domains) keeps its override as
    /// long as all its internals are on the law's support. A law that
    /// overrides this method and may hold internals on their own support
    /// must override `resize_internals_with` as well (or refuse such
    /// internals); otherwise its override is silently bypassed for them.
    fn resize_internals(&mut self, blocks: &[(ElementType, GhostType, usize)]) -> Result<()> {
        // every field is checked before any is resized
        refuse_own_support(self, "resize_internals")?;
        self.apply_internal_restriction()?;
        for name in self.internal_names_sorted() {
            self.for_each_internal_mut(&mut |n, f| {
                if n != name {
                    return Ok(());
                }
                match f {
                    InternalFieldMut::Real(f) => f.resize_blocks(blocks),
                    InternalFieldMut::Int(f) => f.resize_blocks(blocks),
                    InternalFieldMut::UInt(f) => f.resize_blocks(blocks),
                    InternalFieldMut::Bool(f) => f.resize_blocks(blocks),
                }
            })?;
        }
        Ok(())
    }

    /// Upstream `resizeInternals()` driven by the law's element filter:
    /// every internal field, in register-id order, gets
    /// [`InternalField::resize_from_filter`] (blocks of the handled types
    /// of the filter, `filter length · nb_quad` tuples, new tuples drawn by
    /// random fields in upstream order — D-006). Fields first take the
    /// law's [restriction](ConstitutiveLaw::internal_restriction); fields
    /// that still have no [type restriction](InternalField::set_restriction)
    /// (laws without a default) take every type of the filter.
    ///
    /// # Errors
    /// Size overflow or allocation failure.
    fn resize_internals_from_filter(
        &mut self,
        nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
    ) -> Result<()> {
        // every field is checked before any is resized
        refuse_own_support(self, "resize_internals_from_filter")?;
        self.apply_internal_restriction()?;
        let filter = self.element_filter().clone();
        for name in self.internal_names_sorted() {
            self.for_each_internal_mut(&mut |n, f| {
                if n != name {
                    return Ok(());
                }
                match f {
                    InternalFieldMut::Real(f) => f.resize_from_filter(&filter, nb_quad),
                    InternalFieldMut::Int(f) => f.resize_from_filter(&filter, nb_quad),
                    InternalFieldMut::UInt(f) => f.resize_from_filter(&filter, nb_quad),
                    InternalFieldMut::Bool(f) => f.resize_from_filter(&filter, nb_quad),
                }
            })?;
        }
        Ok(())
    }

    /// Upstream `resizeInternals()` for laws whose internals may live on
    /// their own support (CL-001, upstream `registerInternal(id,
    /// nb_component, fe_engine_id[, element_filter])`): every internal, in
    /// register-id order, after taking the law's
    /// [restriction](ConstitutiveLaw::internal_restriction), is resized to
    /// `blocks_of(engine, filter)` — `engine` its
    /// [FE engine](InternalField::fe_engine_id) (`None`: the law's default
    /// engine), `filter` its [own filter](InternalField::element_filter) or
    /// else the law's — returning `(type, ghost, nb_quadrature_points)` for
    /// the blocks of `filter`. Blocks of types a field does not handle are
    /// skipped by the field. For fields on the law's support this is
    /// [`ConstitutiveLaw::resize_internals`] with `blocks_of(None, law
    /// filter)`.
    ///
    /// This default does not go through [`ConstitutiveLaw::resize_internals`]:
    /// a law that overrides one of the two must override the other too (or
    /// refuse internals on their own support), see the note on
    /// `resize_internals`.
    ///
    /// # Errors
    /// Those of `blocks_of` (e.g. an unknown FE engine) and of the resize.
    fn resize_internals_with(&mut self, blocks_of: &InternalBlocksOf<'_>) -> Result<()> {
        self.apply_internal_restriction()?;
        let law_filter = self.element_filter().clone();
        let default_blocks = blocks_of(None, &law_filter)?;
        for name in self.internal_names_sorted() {
            self.for_each_internal_mut(&mut |n, f| {
                if n != name {
                    return Ok(());
                }
                on_field!(f, x => {
                    if x.has_own_support() {
                        let blocks = blocks_of(
                            x.fe_engine_id(),
                            x.element_filter().unwrap_or(&law_filter),
                        )?;
                        x.resize_blocks(&blocks)
                    } else {
                        x.resize_blocks(&default_blocks)
                    }
                })
            })?;
        }
        Ok(())
    }

    /// Upstream `ConstitutiveLaw::addElements`: append every element to the
    /// filter ([`ConstitutiveLaw::add_element`], in list order) and resize
    /// the internals once ([`ConstitutiveLaw::resize_internals_from_filter`]).
    /// Returns the law-local index of each element (what the handler stores
    /// in `constitutive_law_local_numbering`).
    ///
    /// # Errors
    /// Those of [`ConstitutiveLaw::add_element`] and of the resize.
    fn add_elements(
        &mut self,
        elements: &[Element],
        nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
    ) -> Result<Vec<Idx>> {
        let mut local = Vec::with_capacity(elements.len());
        for &e in elements {
            local.push(self.add_element(e)?);
        }
        self.resize_internals_from_filter(nb_quad)?;
        Ok(local)
    }

    /// Add `element` to the filter (upstream `addElement`); returns its
    /// law-local index. Internals are not resized (upstream: the caller
    /// resizes once, `addElements` / `initConstitutiveLaw`).
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] for [`Element::NULL`];
    /// [`Error::CapacityExceeded`] past the local index range.
    fn add_element(&mut self, element: Element) -> Result<Idx> {
        if element.is_null() {
            return Err(Error::invalid("cannot add the null element to a law"));
        }
        let filter = self.element_filter_mut();
        let block = match filter
            .as_map_mut()
            .get_mut(element.ty(), element.ghost_type())
        {
            Some(b) => b,
            None => filter.alloc(0, 1, element.ty(), element.ghost_type(), 0)?,
        };
        let local = akantu_core::try_idx(block.len())?;
        block.try_push_tuple(&[element.index()])?;
        Ok(local)
    }

    /// Compact every internal field after law elements were removed
    /// (upstream `removeIntegrationPoints`, see
    /// [`InternalField::remove_integration_points`]); `new_numbering` is
    /// the law-local renumbering (old index → new, `-1` removed) and
    /// `nb_quad` the law's default engine's point count.
    ///
    /// Internals on their own support (CL-001) are checked first, before
    /// any field changes ([`check_own_support_compaction`]): a
    /// renumbering that would have to compact one is refused; otherwise
    /// they are left as they are (their blocks are not renumbered).
    ///
    /// # Errors
    /// [`Error::Unsupported`] for such an internal; the errors of
    /// [`InternalField::remove_integration_points`].
    fn remove_integration_points(
        &mut self,
        new_numbering: &ElementTypeMapArray<Idx>,
        nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
    ) -> Result<()> {
        check_own_support_compaction(self, new_numbering)?;
        self.for_each_internal_mut(&mut |_, f| {
            if on_field!(&f, x => x.has_own_support()) {
                return Ok(());
            }
            match f {
                InternalFieldMut::Real(f) => f.remove_integration_points(new_numbering, nb_quad),
                InternalFieldMut::Int(f) => f.remove_integration_points(new_numbering, nb_quad),
                InternalFieldMut::UInt(f) => f.remove_integration_points(new_numbering, nb_quad),
                InternalFieldMut::Bool(f) => f.remove_integration_points(new_numbering, nb_quad),
            }
        })
    }

    /// Law part of upstream `ConstitutiveLaw::onElementsRemoved`: drop the
    /// removed elements from the filter, renumber the kept ones with the
    /// mesh renumbering `new_numbering` (old mesh index → new mesh index,
    /// `-1` = removed) and compact the internals. Returns, for every kept
    /// element, `(new mesh element, new law-local index)` — the entries the
    /// handler writes into its `constitutive_law_index` /
    /// `constitutive_law_local_numbering` maps.
    ///
    /// Nothing changes when it fails: the law-local renumbering is planned
    /// and [`check_elements_removed`]'s refusals run before the filter is
    /// rewritten (the models run that check before the mesh commit too).
    ///
    /// # Errors
    /// A kept element renumbered to `-1` (upstream debug assertion) or an
    /// index outside the renumbering; [`Error::Unsupported`] for an
    /// internal on its own support that cannot follow the removal
    /// ([`check_elements_removed`]).
    fn on_elements_removed(
        &mut self,
        removed: &[Element],
        new_numbering: &ElementTypeMapArray<Idx>,
        nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
    ) -> Result<Vec<(Element, Idx)>> {
        // everything that can refuse runs before the filter changes
        // (the own-support refusal used to come after the
        // filter rewrite, leaving the filter compacted and the internals
        // not)
        let plan = plan_law_removal(self, removed, new_numbering)?;
        check_own_support_compaction(self, &plan.law_renumbering)?;
        check_own_filters_unchanged(self, new_numbering)?;
        for (ty, g, f) in plan.new_filters {
            let block = self.element_filter_mut().get_mut(ty, g)?;
            block.copy_from(&Array::from_vec(f, 1)?)?;
        }
        self.remove_integration_points(&plan.law_renumbering, nb_quad)?;
        Ok(plan.kept_entries)
    }
}

/// Read-only model services available to a constitutive law during one
/// phase (replaces upstream's `getModel()` back-reference).
///
/// The model splits itself into `(&mut laws, &services)` for the duration
/// of a phase, so a law — including a Python trampoline calling
/// `self.getModel().getFEEngine()` — can read the model while the law
/// itself is mutably borrowed, without a second mutable borrow of the model.
///
/// Implementations must never wait for the lending thread or for a lock it
/// may hold while closing the phase (in particular the GIL): foreign code
/// reaches the services through a phase-scoped handle whose scope end waits
/// for uses in flight, so such a wait would deadlock. Short locks the lender
/// never holds across the scope end, and `try_*` acquisition, are fine
/// (ARCHITECTURE).
pub trait ModelServices: Sync {
    /// How the laws' per-point loops may run (D-123): the model's
    /// executor; serial by default. A law uses it only for a loop whose
    /// points are independent (the law's declaration, e.g.
    /// `Material::independent_points` of the solid laws).
    fn exec(&self) -> &akantu_core::exec::Exec {
        akantu_core::exec::Exec::serial_ref()
    }
    /// Spatial dimension of the model.
    fn spatial_dimension(&self) -> usize;
    /// The mesh (read-only for the phase).
    fn mesh(&self) -> &Mesh;
    /// Current time step of the default solver.
    fn time_step(&self) -> Real;
    /// Number of integration points per element of `(ty, ghost)`.
    fn nb_integration_points(&self, ty: ElementType, ghost: GhostType) -> usize;
    /// Integration-point coordinates of the elements `filter` of
    /// `(ty, ghost)` into `out` (resized; upstream
    /// `FEEngine::computeIntegrationPointsCoordinates` with a filter).
    fn integration_point_coordinates(
        &self,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()>;
    /// A nodal field by its upstream name (`displacement`, `velocity`, …).
    /// A guard of the (exportable, D-047) array: services lent to a law
    /// hold no element guard themselves, so a law callback may call back
    /// into foreign code (ARCHITECTURE.md, "Shared storage and its aliasing rules") as long as it drops this guard first.
    fn nodal_field(&self, name: &str) -> Result<ArrayRef<'_, Real>>;
    /// The model's FE engine (upstream `getFEEngine(name)`), when the model
    /// has one.
    fn fe_engine(&self, name: &str) -> Result<&FeEngine> {
        Err(Error::not_found("FE engine", name))
    }

    /// Integral of a scalar quadrature field over the elements `filter` of
    /// `(ty, ghost)` (upstream `fem.integrate(f, type, ghost, filter)`:
    /// per-element integrals combined with `Math::reduce`). `f` holds
    /// `filter.len() * nb_quad` values. Default: the default FE engine.
    fn integrate_scalar(
        &self,
        f: &[Real],
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
    ) -> Result<Real> {
        self.fe_engine("")?
            .integrate_scalar(ArrayView::new(f, 1)?, ty, ghost, Some(filter))
    }

    /// Gradient of the nodal field `field` at the quadrature points of the
    /// elements `filter` of `(ty, ghost)` into `out` (upstream
    /// `fem.gradientOnIntegrationPoints(field, out, nb_component, type,
    /// ghost, filter)`: tuples of `nb_component × dim` values, column-major).
    /// Default: the default FE engine.
    fn gradient_on_integration_points(
        &self,
        field: &str,
        out: &mut Array<Real>,
        nb_component: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
    ) -> Result<()> {
        let u = self.nodal_field(field)?;
        self.fe_engine("")?.gradient_on_integration_points(
            self.mesh(),
            u.view(),
            out,
            nb_component,
            ty,
            ghost,
            Some(filter),
        )
    }

    /// [`ModelServices::gradient_on_integration_points`] into a fixed-shape
    /// view (a block of an exportable internal field reached through one
    /// write guard per `(type, ghost)` block): `out` must
    /// already hold `filter.len() · nb_quad` tuples of the gradient's
    /// `nb_component × dim` values (`Error::DimensionMismatch` otherwise);
    /// the values are those of `gradient_on_integration_points`.
    ///
    /// Default: `gradient_on_integration_points` into a temporary array,
    /// then one copy into `out` (so a services type that overrides it keeps
    /// its behaviour; `out` is untouched on an error). Services backed by
    /// the model's FE engine override this method with
    /// [`FeEngine::gradient_on_integration_points_view`] (no temporary, no
    /// copy).
    fn gradient_on_integration_points_view(
        &self,
        field: &str,
        mut out: ArrayViewMut<'_, Real>,
        nb_component: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
    ) -> Result<()> {
        let mut tmp = Array::new(out.nb_component());
        self.gradient_on_integration_points(field, &mut tmp, nb_component, ty, ghost, filter)?;
        if tmp.nb_component() != out.nb_component() || tmp.len() != out.len() {
            return Err(Error::DimensionMismatch {
                context: "gradient_on_integration_points_view: output view (values)",
                expected: tmp.as_slice().len(),
                got: out.as_mut_slice().len(),
            });
        }
        out.as_mut_slice().copy_from_slice(tmp.as_slice());
        Ok(())
    }
}

/// The rank reductions a law **declares** for a model phase.
/// Upstream laws call `comm.allReduce` inside their own methods
/// (e.g. `MaterialDamageIterative::computeAllStresses`: the maximum of the
/// normalised equivalent stress over the ranks). In the port such a phase
/// runs inside a collective operation of the DOF manager (a solver
/// callback, or a model method wrapped in `DofManager::collective`), where
/// a raw communicator call is refused (ARCHITECTURE, one live owner
/// scope per communicator). So a law never calls the communicator there:
/// the model hands it a reducer bound to the running operation's scope
/// right after the phase, and the law issues its reductions through it in
/// upstream's order (every law, in law order, on every rank). The first
/// reduction also agrees the phase's local outcome, so a rank whose phase
/// failed makes every rank fail at the same step.
pub trait LawReducer {
    /// Number of ranks of the model's communicator (upstream
    /// `comm.getNbProc()`; a law reduces only when it is above 1, as
    /// upstream's `allReduce` is the identity on one rank).
    fn nb_ranks(&self) -> usize;

    /// Upstream `comm.allReduce(values, op)` of reals, in place.
    ///
    /// # Errors
    /// The phase failed on this or another rank (agreed), or a transport
    /// failure.
    fn all_reduce_real(&mut self, values: &mut [Real], op: akantu_comm::ReduceOp) -> Result<()>;
}

/// Phase in which a law method runs (bounds the validity of foreign
/// handles to the model, see `akantu-python` scoped handles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LawPhase {
    /// Allocation by the factory (Python `__init__`).
    Construction,
    /// `initMaterial`.
    Init,
    /// `computeStress`.
    Stress,
    /// `computeTangentModuli`.
    Tangent,
    /// Energies.
    Energy,
    /// `beforeSolveStep`.
    BeforeSolveStep,
    /// `afterSolveStep`.
    AfterSolveStep,
}

/// Context handed to every law method: the phase and the model services.
#[derive(Clone, Copy)]
pub struct LawContext<'a> {
    /// Current phase.
    pub phase: LawPhase,
    /// Model services.
    pub services: &'a dyn ModelServices,
}

impl std::fmt::Debug for LawContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LawContext")
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

/// What a factory allocator receives (upstream allocator arguments
/// `(dim, option, model, id)`; the model reference is replaced by the
/// model-specific context `C`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LawSpec {
    /// Spatial dimension.
    pub spatial_dimension: usize,
    /// Option string of the input-file section.
    pub option: String,
    /// Law id (`<model id>:<law kind>:<index>`).
    pub id: String,
}

/// Allocator closure: receives the spec and the model services of the
/// construction phase (upstream allocators receive `(dim, option, model,
/// id)`; a Python constructor `LocalElastic(model, id)` gets a
/// construction-scoped model handle). The services lifetime is
/// higher-ranked, so phase-scoped services borrowing from the model work.
pub type Allocator<L> = Arc<dyn Fn(&LawSpec, &dyn ModelServices) -> Result<Box<L>> + Send + Sync>;

/// Name → allocator registry (upstream `Factory`).
pub struct ConstitutiveLawFactory<L: ?Sized> {
    allocators: BTreeMap<String, Allocator<L>>,
}

impl<L: ?Sized> Default for ConstitutiveLawFactory<L> {
    fn default() -> Self {
        Self {
            allocators: BTreeMap::new(),
        }
    }
}

impl<L: ?Sized> std::fmt::Debug for ConstitutiveLawFactory<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConstitutiveLawFactory")
            .field("registered", &self.allocators.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl<L: ?Sized> ConstitutiveLawFactory<L> {
    /// Register `name` (error if taken, as upstream `registerAllocator`).
    pub fn register(&mut self, name: &str, allocator: Allocator<L>) -> Result<()> {
        if self.allocators.contains_key(name) {
            return Err(Error::AlreadyExists {
                kind: "constitutive law allocator",
                id: name.into(),
            });
        }
        self.allocators.insert(name.into(), allocator);
        Ok(())
    }

    /// Register `name`, replacing an existing allocator (compat switches
    /// that change which law a keyword allocates, e.g. UD-027).
    pub fn replace(&mut self, name: &str, allocator: Allocator<L>) {
        self.allocators.insert(name.into(), allocator);
    }

    /// Create a law by name (upstream `allocate`).
    pub fn create(&self, name: &str, spec: &LawSpec, ctx: &dyn ModelServices) -> Result<Box<L>> {
        let alloc = self
            .allocators
            .get(name)
            .ok_or_else(|| Error::not_found("constitutive law type", name))?;
        alloc(spec, ctx)
    }

    /// Registered names, sorted (upstream `getPossibleAllocators`).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.allocators.keys().map(String::as_str)
    }
}

/// What a selector may read (upstream selectors hold references to the
/// model: its mesh, its `constitutive_law_index` map and its law names).
#[derive(Clone, Copy)]
pub struct SelectorContext<'a> {
    /// The mesh.
    pub mesh: &'a Mesh,
    /// The handler's per-element law index (`-1` = unassigned).
    pub law_index: &'a SharedElementTypeMapArray<Idx>,
    /// Law index by name (upstream `getConstitutiveLawIndex`).
    pub law_by_name: &'a dyn Fn(&str) -> Option<usize>,
}

impl std::fmt::Debug for SelectorContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectorContext").finish_non_exhaustive()
    }
}

/// Chooses the law index of each element (upstream
/// `ConstitutiveLawSelector`, `MeshDataMaterialSelector`,
/// `ElementDataMaterialSelector`, …). `None` defers to the fallback (see
/// [`SelectorChain`]).
pub trait ConstitutiveLawSelector: std::fmt::Debug + Send + Sync {
    /// Law index for `element`, or `None` to defer to the fallback.
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>>;
}

/// Always returns the same index (upstream base `ConstitutiveLawSelector`
/// with its `fallback_value`, default 0).
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultSelector(pub usize);

impl ConstitutiveLawSelector for DefaultSelector {
    fn select(&self, _ctx: &SelectorContext<'_>, _element: Element) -> Result<Option<usize>> {
        Ok(Some(self.0))
    }
}

/// Upstream `DefaultConstitutiveLawSelector` (`DefaultMaterialSelector`):
/// the index already stored in the handler's `constitutive_law_index` map
/// (e.g. set by the user before `initFull`), `None` when the element has
/// none (`-1`, missing block or index past the block).
#[derive(Clone, Copy, Debug, Default)]
pub struct LawIndexSelector;

impl ConstitutiveLawSelector for LawIndexSelector {
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        // absent block → no law; any other error (a protocol violation of
        // an exported map) is returned
        let Some(block) =
            akantu_core::error::if_present(ctx.law_index.get(element.ty(), element.ghost_type()))?
        else {
            return Ok(None);
        };
        match block.as_slice().get(element.idx()) {
            Some(&i) if i != -1 => Ok(Some(akantu_core::try_to_usize(i)?)),
            _ => Ok(None),
        }
    }
}

/// Upstream `ElementDataConstitutiveLawSelector<T>` /
/// `MeshDataConstitutiveLawSelector<T>` (`MeshDataMaterialSelector`): the
/// law is read from the element data `name` of the mesh — a law **name**
/// for string data (`physical_names`), the law **index itself** for integer
/// data (upstream stores a `first_index` but never uses it). Missing data
/// or an unknown name defer to the fallback (upstream catches the
/// exception).
#[derive(Clone, Debug)]
pub struct ElementDataSelector {
    /// Element data name (e.g. `physical_names`, `tag_0`).
    pub name: String,
}

impl ElementDataSelector {
    /// Selector on the element data `name`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl ConstitutiveLawSelector for ElementDataSelector {
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        let Ok(data) = ctx.mesh.data().elemental(&self.name) else {
            return Ok(None);
        };
        let index = |v: i64| -> Result<Option<usize>> {
            // Upstream would index its law vector with a negative value.
            usize::try_from(v)
                .map(Some)
                .map_err(|_| Error::InvalidIndex {
                    what: "law index from element data",
                    value: v,
                })
        };
        match data {
            MeshDataValues::String(m) => Ok(m
                .element(element)
                .ok()
                .and_then(|v| v.first())
                .and_then(|name| (ctx.law_by_name)(name))),
            MeshDataValues::Int(m) => match first_value(m, element)? {
                Some(v) => index(i64::from(v)),
                None => Ok(None),
            },
            MeshDataValues::UInt(m) => match first_value(m, element)? {
                Some(v) => index(i64::from(v)),
                None => Ok(None),
            },
            _ => Ok(None),
        }
    }
}

/// First value of `element`'s tuple in the exportable map `m`, `None` when
/// upstream's lookup would throw (and its selector catch): absent block,
/// `ElementNull`, index past the block. A protocol violation of an exported
/// block is returned, never taken for absence (guide
/// P13).
fn first_value<T: Copy>(m: &SharedElementTypeMapArray<T>, element: Element) -> Result<Option<T>> {
    let Some(block) = akantu_core::error::if_present(m.get(element.ty(), element.ghost_type()))?
    else {
        return Ok(None);
    };
    Ok(element
        .try_idx()
        .and_then(|i| block.get_tuple(i).ok())
        .and_then(|t| t.first().copied()))
}

/// Upstream fallback chain: the selectors are asked in order; the first
/// `Some` wins; otherwise `fallback_value` (upstream
/// `setFallback(selector)` / `setFallback(value)`, default 0).
#[derive(Debug, Default)]
pub struct SelectorChain {
    /// Selectors, first asked first.
    pub selectors: Vec<Box<dyn ConstitutiveLawSelector>>,
    /// Value when every selector deferred.
    pub fallback_value: usize,
}

impl SelectorChain {
    /// Upstream default of a constitutive-laws handler:
    /// `DefaultConstitutiveLawSelector(constitutive_law_index)` with
    /// fallback value 0.
    #[must_use]
    pub fn upstream_default() -> Self {
        Self {
            selectors: vec![Box::new(LawIndexSelector)],
            fallback_value: 0,
        }
    }

    /// A user selector with the upstream default chain as fallback
    /// (`selector->setFallback(model.getConstitutiveLawSelector())`).
    #[must_use]
    pub fn with_fallback_to_default(selector: Box<dyn ConstitutiveLawSelector>) -> Self {
        Self {
            selectors: vec![selector, Box::new(LawIndexSelector)],
            fallback_value: 0,
        }
    }

    /// Law index of `element`.
    pub fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<usize> {
        for s in &self.selectors {
            if let Some(i) = s.select(ctx, element)? {
                return Ok(i);
            }
        }
        Ok(self.fallback_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait Law: std::fmt::Debug {
        fn tag(&self) -> String;
    }

    #[derive(Debug)]
    struct Elastic(usize);
    impl Law for Elastic {
        fn tag(&self) -> String {
            format!("elastic{}", self.0)
        }
    }

    struct NoModel(Mesh);
    impl ModelServices for NoModel {
        fn spatial_dimension(&self) -> usize {
            3
        }
        fn mesh(&self) -> &Mesh {
            &self.0
        }
        fn time_step(&self) -> Real {
            0.0
        }
        fn nb_integration_points(&self, _: ElementType, _: GhostType) -> usize {
            0
        }
        fn integration_point_coordinates(
            &self,
            _: ElementType,
            _: GhostType,
            _: &[Idx],
            _: &mut Array<Real>,
        ) -> Result<()> {
            Ok(())
        }
        fn nodal_field(&self, name: &str) -> Result<ArrayRef<'_, Real>> {
            Err(Error::not_found("nodal field", name))
        }
    }

    #[test]
    fn factory_registration_and_creation() {
        let mut f: ConstitutiveLawFactory<dyn Law> = ConstitutiveLawFactory::default();
        f.register(
            "elastic",
            Arc::new(|s: &LawSpec, _: &dyn ModelServices| {
                Ok(Box::new(Elastic(s.spatial_dimension)) as Box<dyn Law>)
            }),
        )
        .unwrap();
        assert!(
            f.register(
                "elastic",
                Arc::new(|_: &LawSpec, _: &dyn ModelServices| Err(Error::invalid("x")))
            )
            .is_err()
        );
        let spec = LawSpec {
            spatial_dimension: 3,
            option: String::new(),
            id: "smm:material:0".into(),
        };
        let model = NoModel(Mesh::new(3, "m").unwrap());
        assert_eq!(
            f.create("elastic", &spec, &model).unwrap().tag(),
            "elastic3"
        );
        assert!(f.create("plastic", &spec, &model).is_err());
        assert_eq!(f.names().collect::<Vec<_>>(), vec!["elastic"]);
    }

    #[test]
    fn selectors_and_fallback_chain() {
        use akantu_mesh::MeshAccessor;
        let mut mesh = Mesh::new(2, "m").unwrap();
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for x in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
                a.push_node(&x).unwrap();
            }
            let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
            c.push_tuple(&[0, 1, 2]);
            c.push_tuple(&[1, 3, 2]);
        }
        let mut names = ElementTypeMapArray::<String>::new("physical_names");
        names
            .as_map_mut()
            .insert(
                ElementType::Triangle3,
                GhostType::NotGhost,
                Array::from_vec(vec!["steel".to_owned(), "unknown".to_owned()], 1).unwrap(),
            )
            .unwrap();
        mesh.data_mut()
            .insert_elemental("physical_names", MeshDataValues::String(names));
        let mut law_index = SharedElementTypeMapArray::<Idx>::new("constitutive_law index");
        law_index
            .alloc(2, 1, ElementType::Triangle3, GhostType::NotGhost, -1)
            .unwrap();
        law_index
            .get_mut(ElementType::Triangle3, GhostType::NotGhost)
            .unwrap()
            .as_mut_slice()[1] = 3;
        let by_name = |n: &str| (n == "steel").then_some(1);
        let ctx = SelectorContext {
            mesh: &mesh,
            law_index: &law_index,
            law_by_name: &by_name,
        };
        let e0 = Element::new(ElementType::Triangle3, 0, GhostType::NotGhost);
        let e1 = Element::new(ElementType::Triangle3, 1, GhostType::NotGhost);
        assert_eq!(DefaultSelector(2).select(&ctx, e0).unwrap(), Some(2));
        assert_eq!(LawIndexSelector.select(&ctx, e0).unwrap(), None);
        assert_eq!(LawIndexSelector.select(&ctx, e1).unwrap(), Some(3));
        let data = ElementDataSelector::new("physical_names");
        assert_eq!(data.select(&ctx, e0).unwrap(), Some(1));
        assert_eq!(data.select(&ctx, e1).unwrap(), None, "unknown name");
        assert_eq!(
            ElementDataSelector::new("missing")
                .select(&ctx, e0)
                .unwrap(),
            None
        );
        let chain = SelectorChain::with_fallback_to_default(Box::new(data));
        assert_eq!(chain.select(&ctx, e0).unwrap(), 1);
        assert_eq!(chain.select(&ctx, e1).unwrap(), 3);
        let chain = SelectorChain::upstream_default();
        assert_eq!(chain.select(&ctx, e0).unwrap(), 0);
    }
}
