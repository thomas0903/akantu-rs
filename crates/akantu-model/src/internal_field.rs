// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/constitutive_laws/
// {internal_field.hh, internal_field_tmpl.hh, random_internal_field.hh,
// random_internal_field_tmpl.hh}, constitutive_law_tmpl.hh
// (flattenInternal, inflateInternal).

//! Internal fields: per-quadrature-point state of constitutive laws.
//!
//! Layout: one `Array<T>` per `(type, ghost)` with
//! `nb_filtered_elements * nb_quad` tuples of `nb_component` values,
//! element-major (upstream identical). Optional history (`previous`) is
//! saved/restored explicitly (upstream `saveCurrentValues` /
//! `restorePreviousValues`).
//!
//! # Upstream semantics reproduced
//!
//! * **Default value.** New tuples created by [`InternalField::resize`] get
//!   the field's default value; [`InternalField::set_default_value`] changes
//!   it **and resets every existing value** (upstream `setDefaultValue` →
//!   `reset()`), e.g. a `.dat` parameter `delta_T = 10` of a thermal law.
//! * **History.** [`InternalField::initialize_history`] creates the
//!   previous values with the current block sizes, filled with the default
//!   value at that moment (upstream copy-constructs `previous_<id>` and
//!   `reset()`s it — the current values are *not* copied). The history keeps
//!   its own default value afterwards (upstream: a separate field).
//! * **Deferred saves** (port-specific; the same values as upstream's
//!   copy). [`InternalField::save_current_values`] does not copy a block
//!   whose current values are not exported (no NumPy view, D-047): it
//!   records that the previous values *are* the current ones. The copy is
//!   made when the current values are about to change (every mutable
//!   accessor, resize, reset, compaction); it is skipped when nothing
//!   changes them before the next save, and it becomes an exchange of the
//!   two allocations when the caller announces that it rewrites the whole
//!   block ([`InternalField::prepare_overwrite`],
//!   [`InternalField::commit_overwrite`]). [`InternalField::previous`]
//!   returns the current values of such a block, which are exactly what the
//!   copy would have held. A block exported at the save is copied at once
//!   (Python may write it at any time), and a block can only be exported
//!   through [`InternalField::values_mut`], which performs the deferred
//!   saves first, so a view never sees an exchanged allocation; previous
//!   values reach Python only as copies. Upstream copies at every
//!   `saveCurrentValues`.
//! * **Random fields** ([`InternalField::new_random`], upstream
//!   `RandomInternalField<Real>`): every value written by `reset()` or by
//!   `resize()` (only the new tail of each block) is a draw of the field's
//!   [`RandomParameter`] from the process-global engine
//!   (`RandomGenerator<Idx>`), in upstream order — ghost types `NotGhost`
//!   then `Ghost`, element types in enum order, flat storage order inside a
//!   block (decision D-006, `akantu_core::random` draw-order contract). The
//!   history of a random field is a plain field with default 0 (upstream:
//!   the base `InternalField::default_value` is never set for random
//!   fields).
//! * **Type restriction** (upstream `spatial_dimension` / `element_kind`,
//!   row CL-002). A field restricted with
//!   [`InternalField::set_restriction`] (law dimension, `_ek_regular` by
//!   default; [`InternalField::set_element_kind`] is upstream
//!   `setElementKind`) only holds and updates blocks of element types of
//!   that dimension and kind: [`InternalField::resize_from_filter`]
//!   allocates one block per such type of the law's element filter (empty
//!   filter blocks included), sized `filter length · nb_quad`, and
//!   [`InternalField::reset`] / [`InternalField::resize_from_filter`] skip
//!   every other block. A field of another kind than its filter's types
//!   (e.g. `_ek_cohesive` on a regular filter) therefore never draws. The
//!   explicit-block resizes ([`InternalField::resize`],
//!   [`InternalField::resize_blocks`]) honour the restriction too: a
//!   restricted field never allocates a block of a type it does not handle.
//!   Laws restrict their fields by default
//!   ([`InternalField::adopt_restriction`] with the law's
//!   `ConstitutiveLaw::internal_restriction`, applied by every law-level
//!   resize).
//!   Upstream allocates blocks for the FE engine's element dimension but
//!   iterates the field's own dimension; the two only differ for a field
//!   constructed with an explicit dimension, which `registerInternal` never
//!   does — the port uses the field's dimension for both. Upstream keeps the
//!   zero-size blocks a field allocated before its kind was changed; the
//!   port has no block there (no value either way; UD-500).
//! * **Element removal.** [`InternalField::remove_integration_points`]
//!   compacts the blocks with a law-local renumbering (upstream
//!   `removeIntegrationPoints`). Upstream does not compact the history
//!   (`previous_<id>` is not a registered internal), so a
//!   `restorePreviousState` before the next save would restore blocks of the
//!   old size; the port compacts the history too (UD-106).
//!   The renumbering must map the kept elements onto `0..kept` exactly
//!   once each.

use akantu_core::array_shared::{ArrayMut, ArrayRef};
use akantu_core::random::RandomParameter;
use akantu_core::{
    Array, ElementKind, ElementType, ElementTypeMap, ElementTypeMapArray, Error, GhostType, Id,
    Idx, Real, Result, SharedArray, SharedElementTypeMapArray, TypeFilter,
};

/// Element types an internal field handles (upstream
/// `InternalField::spatial_dimension` and `element_kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeRestriction {
    /// Spatial dimension of the handled types (`None`: any).
    pub spatial_dimension: Option<usize>,
    /// Kind of the handled types (upstream default `_ek_regular`).
    pub element_kind: ElementKind,
}

impl TypeRestriction {
    /// `true` if blocks of `ty` belong to the field.
    #[must_use]
    pub fn handles(&self, ty: ElementType) -> bool {
        ty.kind() == self.element_kind
            && self
                .spatial_dimension
                .is_none_or(|d| ty.spatial_dimension() == d)
    }
}

/// Copy the current block `a` into `dst` **in place** when the block sizes
/// agree (upstream `ElementTypeMapArray::copy` → `Array::copy`).
fn save_block<T: Clone>(
    dst: &mut ElementTypeMapArray<T>,
    a: &SharedArray<T>,
    ty: ElementType,
    ghost: GhostType,
) -> Result<()> {
    let a = a.read()?;
    match dst.as_map_mut().get_mut(ty, ghost) {
        Some(d) => d.copy_from(&a)?,
        None => {
            dst.as_map_mut().insert(ty, ghost, (*a).clone())?;
        }
    }
    Ok(())
}

/// `dst ← src` for the current (exportable) values, in place when the
/// block sizes agree, so foreign views of `dst` stay attached across
/// `save`/`restore`; blocks listed in `pending` (deferred saves) already
/// hold these values.
fn restore_blocks<T: Clone>(
    dst: &mut SharedElementTypeMapArray<T>,
    src: &ElementTypeMapArray<T>,
    pending: &[(ElementType, GhostType)],
) -> Result<()> {
    for (ty, ghost, a) in src.as_map().iter_all() {
        if pending.contains(&(ty, ghost)) {
            continue;
        }
        match dst.as_map_mut().get_mut(ty, ghost) {
            Some(d) => d.write()?.copy_from(a)?,
            None => {
                dst.as_map_mut()
                    .insert(ty, ghost, SharedArray::new(a.clone()))?;
            }
        }
    }
    Ok(())
}

/// How values are written by `reset()`/`resize()` (upstream
/// `setArrayValues`).
#[derive(Clone, Debug)]
enum Filler<T> {
    /// Plain field: the default value.
    Default,
    /// Random field (`RandomInternalField<Real>`): draws of the parameter.
    Random {
        param: Box<RandomParameter>,
        fill: fn(&mut RandomParameter, &mut [T]),
        set_base: fn(&mut RandomParameter, &T),
    },
}

/// Draws into a `Real` slice from the global `RandomGenerator<Idx>` engine
/// (upstream `RandomParameter::setValues<RandomGenerator>`).
fn random_fill(param: &mut RandomParameter, out: &mut [Real]) {
    param.set_values(out);
}

/// Upstream `RandomInternalField::setDefaultValue`: the base value.
fn random_set_base(param: &mut RandomParameter, value: &Real) {
    param.set_base_value(*value);
}

/// The history of a field (upstream `previous_values`, a separate
/// `InternalField` with its own default value).
#[derive(Clone, Debug)]
struct History<T> {
    values: ElementTypeMapArray<T>,
    default_value: T,
    /// Blocks whose save is deferred (see "Deferred saves" in the module
    /// documentation): their previous values are the current values, and
    /// `values` holds nothing meaningful for them. Invariants: the current
    /// block is not exported, and both blocks exist with the same number of
    /// tuples and components.
    pending: Vec<(ElementType, GhostType)>,
}

impl<T: Clone> History<T> {
    fn is_pending(&self, ty: ElementType, ghost: GhostType) -> bool {
        self.pending.contains(&(ty, ghost))
    }

    /// Perform the deferred save of `(ty, ghost)` (`previous <- current`, a
    /// copy), if there is one.
    fn materialize(
        &mut self,
        current: &SharedElementTypeMapArray<T>,
        ty: ElementType,
        ghost: GhostType,
    ) {
        let Some(i) = self.pending.iter().position(|&b| b == (ty, ghost)) else {
            return;
        };
        self.pending.swap_remove(i);
        let (Ok(src), Some(dst)) = (
            current.shared(ty, ghost),
            self.values.as_map_mut().get_mut(ty, ghost),
        ) else {
            return; // not reachable: both blocks exist while pending
        };
        // not exported while pending: the guard counts nothing
        let src = src.borrow();
        if dst.len() == src.len() && dst.nb_component() == src.nb_component() {
            dst.as_mut_slice().clone_from_slice(src.as_slice());
        } else {
            // not reachable (same shapes while pending); a full copy keeps
            // the result right anyway
            let id = dst.id().to_owned();
            *dst = (*src).clone();
            dst.set_id(id);
        }
    }

    /// Perform every deferred save.
    fn materialize_all(&mut self, current: &SharedElementTypeMapArray<T>) {
        while let Some(&(ty, ghost)) = self.pending.last() {
            self.materialize(current, ty, ghost);
        }
    }
}

/// Per-quadrature-point field with optional history.
#[derive(Clone, Debug)]
pub struct InternalField<T> {
    name: Id,
    nb_component: usize,
    default_value: T,
    filler: Filler<T>,
    // Boxed: an `ElementTypeMapArray` is a dense ~4 KiB table, and laws hold
    // many fields (keeps law values small enough to move on the stack).
    // Exportable (D-047): Python `getInternalReal(...)` views alias it.
    values: Box<SharedElementTypeMapArray<T>>,
    previous: Option<Box<History<T>>>,
    /// Handled element types (`None`: every block, the explicit-block API).
    restriction: Option<TypeRestriction>,
    /// Support of the field (upstream `registerInternal(id, nb_component,
    /// fe_engine_id[, element_filter])`, CL-001); `None`: the law's.
    support: Option<Box<FieldSupport>>,
}

/// The FE engine and element filter of an internal field registered on
/// another support than its law's (upstream `registerInternal(id,
/// nb_component, fe_engine_id, element_filter)`,
/// `constitutive_law_tmpl.hh:56-72`; see
/// [`InternalField::set_fe_engine_id`]).
#[derive(Clone, Debug, Default)]
pub struct FieldSupport {
    /// FE engine whose integration points the field lives on (`None`: the
    /// law's default engine).
    pub fe_engine_id: Option<Id>,
    /// Element filter of the field (`None`: the law's). Upstream keeps a
    /// reference to a filter the law owns; the port's field owns a copy,
    /// which the law updates with [`InternalField::element_filter_mut`]
    /// when its own filter changes.
    pub element_filter: Option<ElementTypeMapArray<Idx>>,
}

impl<T: Clone> InternalField<T> {
    /// New empty field (upstream `registerInternal(id, nb_component)`).
    pub fn new(name: impl Into<Id>, nb_component: usize, default_value: T) -> Result<Self> {
        if nb_component == 0 {
            return Err(Error::invalid(
                "internal field needs at least one component",
            ));
        }
        let name = name.into();
        Ok(Self {
            values: Box::new(SharedElementTypeMapArray::new(name.clone())),
            name,
            nb_component,
            default_value,
            filler: Filler::Default,
            previous: None,
            restriction: None,
            support: None,
        })
    }

    /// Upstream `registerInternal(id, nb_component, fe_engine_id)`
    /// (CL-001): the field lives on the integration points of the model's
    /// FE engine `fe_engine_id` instead of the law's default engine (the
    /// `nb_quad` of its blocks comes from that engine when the law resizes
    /// with [`crate::constitutive_law::ConstitutiveLaw::resize_internals_with`]).
    /// Upstream laws only pass their own default engine (the reinforcement
    /// passes its `EmbeddedInterfaceFEEngine`, `material_reinforcement_tmpl.hh:
    /// 50-53, 253`); this is the API for user laws.
    pub fn set_fe_engine_id(&mut self, fe_engine_id: impl Into<Id>) {
        self.support.get_or_insert_with(Box::default).fe_engine_id = Some(fe_engine_id.into());
    }

    /// Upstream `registerInternal(id, nb_component, fe_engine_id,
    /// element_filter)` (CL-001): the field is sized on `filter` (law-local
    /// index → mesh element, per type and ghost type) instead of the law's
    /// element filter; see [`FieldSupport::element_filter`].
    pub fn set_element_filter(&mut self, filter: ElementTypeMapArray<Idx>) {
        self.support.get_or_insert_with(Box::default).element_filter = Some(filter);
    }

    /// The FE engine of the field (`None`: the law's default engine).
    #[must_use]
    pub fn fe_engine_id(&self) -> Option<&str> {
        self.support
            .as_ref()
            .and_then(|s| s.fe_engine_id.as_deref())
    }

    /// The field's own element filter (`None`: the law's).
    #[must_use]
    pub fn element_filter(&self) -> Option<&ElementTypeMapArray<Idx>> {
        self.support
            .as_ref()
            .and_then(|s| s.element_filter.as_ref())
    }

    /// The field's own element filter, mutably (`None`: it uses the
    /// law's).
    pub fn element_filter_mut(&mut self) -> Option<&mut ElementTypeMapArray<Idx>> {
        self.support
            .as_mut()
            .and_then(|s| s.element_filter.as_mut())
    }

    /// `true` if the field has its own FE engine or element filter
    /// ([`InternalField::set_fe_engine_id`],
    /// [`InternalField::set_element_filter`]).
    #[must_use]
    pub fn has_own_support(&self) -> bool {
        self.support.is_some()
    }

    /// Restrict the field to the element types of `spatial_dimension` and
    /// `element_kind` (upstream: the law's dimension at construction, the
    /// kind `_ek_regular` unless changed with `setElementKind`); see the
    /// module documentation.
    pub fn set_restriction(&mut self, spatial_dimension: usize, element_kind: ElementKind) {
        self.restriction = Some(TypeRestriction {
            spatial_dimension: Some(spatial_dimension),
            element_kind,
        });
    }

    /// Upstream `setElementKind`: change the handled kind (any dimension
    /// when no restriction was set before).
    pub fn set_element_kind(&mut self, element_kind: ElementKind) {
        let spatial_dimension = self.restriction.and_then(|r| r.spatial_dimension);
        self.restriction = Some(TypeRestriction {
            spatial_dimension,
            element_kind,
        });
    }

    /// Handled element types (`None`: unrestricted).
    #[must_use]
    pub fn restriction(&self) -> Option<TypeRestriction> {
        self.restriction
    }

    /// Take the law's restriction as the default (upstream: an
    /// `InternalField` is constructed with the law's spatial dimension and
    /// `_ek_regular`): an unrestricted field gets `law`; a field whose kind
    /// was set without a dimension ([`InternalField::set_element_kind`] on
    /// an unrestricted field, upstream `setElementKind`) keeps its kind and
    /// gets the law's dimension; a field with a dimension keeps its own
    /// restriction.
    pub fn adopt_restriction(&mut self, law: TypeRestriction) {
        self.restriction = Some(match self.restriction {
            None => law,
            Some(own) => TypeRestriction {
                spatial_dimension: own.spatial_dimension.or(law.spatial_dimension),
                element_kind: own.element_kind,
            },
        });
    }

    /// `true` if the field handles blocks of `ty`.
    #[must_use]
    pub fn handles(&self, ty: ElementType) -> bool {
        self.restriction.is_none_or(|r| r.handles(ty))
    }

    /// Upstream `InternalField::resize()` over the law's element filter:
    /// for both ghost types (not-ghost first) and every type of `filter`
    /// the field [handles](InternalField::handles), in enum order, the block
    /// gets `filter(type, ghost).len() · nb_quad(type, ghost)` tuples — new
    /// tuples take the default value, or draws for a random field (the
    /// upstream draw order), removed tuples are dropped; the history is
    /// resized alike. Blocks of types the field does not handle are left
    /// alone.
    ///
    /// # Errors
    /// Size overflow or allocation failure.
    pub fn resize_from_filter(
        &mut self,
        filter: &ElementTypeMapArray<Idx>,
        nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
    ) -> Result<()> {
        for ghost in GhostType::ALL {
            let types: Vec<ElementType> = filter
                .element_types(TypeFilter {
                    spatial_dimension: None,
                    ghost_type: ghost,
                    kind: None,
                })
                .filter(|&ty| self.handles(ty))
                .collect();
            for ty in types {
                let n = filter.get(ty, ghost)?.len();
                let size = akantu_core::checked_len(n, nb_quad(ty, ghost), "internal field")?;
                self.resize(ty, ghost, size)?;
            }
        }
        Ok(())
    }

    /// Name (upstream register id).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Components per quadrature point.
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.nb_component
    }

    /// Default value of new entries (upstream `operator T()` of a plain
    /// field; for a random field the plain default, see
    /// [`InternalField::random_parameter`]).
    #[must_use]
    pub fn default_value(&self) -> &T {
        &self.default_value
    }

    /// `true` for a random field.
    #[must_use]
    pub fn is_random(&self) -> bool {
        matches!(self.filler, Filler::Random { .. })
    }

    /// Enable history (upstream `initializeHistory`): previous values with
    /// the current block sizes, filled with the current default value.
    pub fn initialize_history(&mut self) {
        if self.previous.is_some() {
            return;
        }
        let mut values = ElementTypeMapArray::new(format!("previous_{}", self.name));
        for (ty, ghost, a) in self.values.as_map().iter_all() {
            // Same size as an existing block: cannot exceed checked limits.
            let mut p = Array::filled(a.len(), self.nb_component, self.default_value.clone());
            let suffix = if ghost == GhostType::Ghost {
                ":ghost"
            } else {
                ""
            };
            p.set_id(format!("previous_{}:{ty}{suffix}", self.name));
            // The block does not exist yet in a fresh map.
            let _ = values.as_map_mut().insert(ty, ghost, p);
        }
        self.previous = Some(Box::new(History {
            values,
            default_value: self.default_value.clone(),
            pending: Vec::new(),
        }));
    }

    /// `true` if history is kept.
    #[must_use]
    pub fn has_history(&self) -> bool {
        self.previous.is_some()
    }

    /// Write the filler into `a[from..]` (upstream `setArrayValues`).
    fn fill_tail(filler: &mut Filler<T>, default: &T, a: &mut ArrayMut<'_, T>, from_tuple: usize) {
        let nbc = a.nb_component();
        let start = (from_tuple * nbc).min(a.as_slice().len());
        let tail = &mut a.as_mut_slice()[start..];
        match filler {
            Filler::Default => tail.fill(default.clone()),
            Filler::Random { param, fill, .. } => fill(param, tail),
        }
    }

    /// Size the block `(ty, ghost)` to `nb_quad_points` tuples; new tuples
    /// get the default value (random fields: draws) — upstream `resize` for
    /// one block. To reproduce upstream's draw order over several blocks use
    /// [`InternalField::resize_blocks`]. A restricted field ignores a type it
    /// does not [handle](InternalField::handles): no block is allocated and
    /// nothing is drawn (upstream iterates the handled types only).
    pub fn resize(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        nb_quad_points: usize,
    ) -> Result<()> {
        if !self.handles(ty) {
            return Ok(());
        }
        self.materialize(ty, ghost);
        let old = self.values.shared(ty, ghost).map_or(0, SharedArray::len);
        let mut a = self.values.alloc(
            nb_quad_points,
            self.nb_component,
            ty,
            ghost,
            self.default_value.clone(),
        )?;
        if matches!(self.filler, Filler::Random { .. }) && nb_quad_points > old {
            Self::fill_tail(&mut self.filler, &self.default_value, &mut a, old);
        }
        if let Some(p) = &mut self.previous {
            p.values.alloc(
                nb_quad_points,
                self.nb_component,
                ty,
                ghost,
                p.default_value.clone(),
            )?;
        }
        Ok(())
    }

    /// Resize several blocks `(type, ghost, nb_quad_points)` in upstream
    /// order (ghost type `NotGhost` first, element types in enum order),
    /// which fixes the draw order of random fields (upstream `resize()`
    /// over the element filter). Blocks of types a restricted field does
    /// not handle are skipped (see [`InternalField::resize`]).
    pub fn resize_blocks(&mut self, blocks: &[(ElementType, GhostType, usize)]) -> Result<()> {
        let mut sorted: Vec<_> = blocks.to_vec();
        sorted.sort_by_key(|&(ty, g, _)| (g.index(), ty.index()));
        for (ty, g, n) in sorted {
            self.resize(ty, g, n)?;
        }
        Ok(())
    }

    /// Rewrite every value with the filler (upstream `reset()`): the default
    /// value, or fresh draws for a random field, in upstream order (ghost
    /// types not-ghost first, element types in enum order); a restricted
    /// field only rewrites the blocks it handles.
    ///
    /// # Panics
    /// If a block is exported and no native section of its runtime is
    /// active (protocol violation, see [`SharedArray::borrow_mut`]).
    pub fn reset(&mut self) {
        self.materialize_all();
        let default = self.default_value.clone();
        let restriction = self.restriction;
        for g in GhostType::ALL {
            for (ty, a) in self.values.as_map_mut().iter_mut(g) {
                if restriction.is_none_or(|r| r.handles(ty)) {
                    Self::fill_tail(&mut self.filler, &default, &mut a.borrow_mut(), 0);
                }
            }
        }
    }

    /// Upstream `setDefaultValue(value)`: new default **and** `reset()` of
    /// all current values (not of the history). For a random field the
    /// value becomes the base value of its random parameter (upstream
    /// `RandomInternalField::setDefaultValue`).
    pub fn set_default_value(&mut self, value: T) {
        match &mut self.filler {
            Filler::Default => self.default_value = value,
            Filler::Random {
                param, set_base, ..
            } => set_base(param, &value),
        }
        self.reset();
    }

    /// Current values of `(ty, ghost)` (a guard, ARCHITECTURE.md, "Shared storage and its aliasing rules").
    pub fn get(&self, ty: ElementType, ghost: GhostType) -> Result<ArrayRef<'_, T>> {
        self.values.get(ty, ghost)
    }

    /// Mutable current values (performs a deferred save of the block
    /// first).
    pub fn get_mut(&mut self, ty: ElementType, ghost: GhostType) -> Result<ArrayMut<'_, T>> {
        self.materialize(ty, ghost);
        self.values.get_mut(ty, ghost)
    }

    /// Announce that the caller rewrites **every** value of the current
    /// block `(ty, ghost)` before it reads any of them, and cannot fail in
    /// between (it checks its inputs first). A deferred save of the block
    /// then becomes an exchange of the current and previous allocations (no
    /// copy), and the current values of the block are unspecified until the
    /// caller has written them. Without a deferred save of the block, or
    /// when the exchange is not possible, this performs the save as
    /// [`InternalField::get_mut`] would.
    pub fn prepare_overwrite(&mut self, ty: ElementType, ghost: GhostType) {
        let Some(p) = &mut self.previous else {
            return;
        };
        let Some(i) = p.pending.iter().position(|&b| b == (ty, ghost)) else {
            return;
        };
        let swapped = match (
            self.values.shared_mut(ty, ghost),
            p.values.as_map_mut().get_mut(ty, ghost),
        ) {
            (Ok(c), Some(prev)) => c.try_swap_values(prev),
            _ => false,
        };
        if swapped {
            p.pending.swap_remove(i);
        } else {
            p.materialize(&self.values, ty, ghost);
        }
    }

    /// `current(ty, ghost) <- new` for a whole block (`new` has the shape of
    /// the current block); the values left in `new` are unspecified. The
    /// result is that of `get_mut(ty, ghost)?.copy_from(new)`, obtained by
    /// exchanging allocations where possible: a deferred save of the block
    /// and the write then cost no copy. Laws that compute a block into
    /// scratch storage of their own commit it with this; nothing changes
    /// if it fails.
    ///
    /// # Errors
    /// A missing block, `new` of another shape
    /// ([`Error::DimensionMismatch`]), the guard errors of
    /// [`InternalField::get_mut`].
    pub fn commit_overwrite(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        new: &mut Array<T>,
    ) -> Result<()> {
        {
            let c = self.values.shared(ty, ghost)?;
            if c.len() != new.len() || c.nb_component() != new.nb_component() {
                return Err(Error::DimensionMismatch {
                    context: "InternalField::commit_overwrite",
                    expected: c.len(),
                    got: new.len(),
                });
            }
        }
        self.prepare_overwrite(ty, ghost);
        let c = self.values.shared_mut(ty, ghost)?;
        if !c.try_swap_values(new) {
            c.write()?.copy_from(new)?;
        }
        Ok(())
    }

    /// `true` if the block `(ty, ghost)` exists.
    #[must_use]
    pub fn exists(&self, ty: ElementType, ghost: GhostType) -> bool {
        self.values.exists(ty, ghost)
    }

    /// Previous values of `(ty, ghost)` (upstream `previous(type, ghost)`).
    /// For a block with a deferred save these are the current values (the
    /// returned array then carries the current block's id).
    pub fn previous(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<T>> {
        let p = self.previous.as_ref().ok_or_else(|| {
            Error::invalid(format!("internal field '{}' has no history", self.name))
        })?;
        if p.is_pending(ty, ghost) {
            return self
                .values
                .shared(ty, ghost)?
                .as_unexported()
                .ok_or_else(|| {
                    Error::invalid(format!(
                        "internal field '{}': deferred save of an exported block",
                        self.name
                    ))
                });
        }
        p.values.get(ty, ghost)
    }

    /// Mutable previous values of `(ty, ghost)`, allocated (empty, with the
    /// history default) if the block does not exist yet (upstream writes
    /// `previous(type, ghost)` directly, e.g. the cohesive laws' insertion
    /// check pre-sizes and fills `tractions.previous` for elements about
    /// to be inserted; the phase-field laws initialise `previous_phi` in
    /// `updateInternalParameters`, on blocks that already exist, so
    /// they are unaffected by the allocation).
    pub fn previous_mut(&mut self, ty: ElementType, ghost: GhostType) -> Result<&mut Array<T>> {
        self.materialize(ty, ghost);
        let nbc = self.nb_component;
        let name = &self.name;
        let p = self
            .previous
            .as_mut()
            .ok_or_else(|| Error::invalid(format!("internal field '{name}' has no history")))?;
        if !p.values.exists(ty, ghost) {
            let d = p.default_value.clone();
            p.values.alloc(0, nbc, ty, ghost, d)?;
        }
        p.values.get_mut(ty, ghost)
    }

    /// Current values and previous values of `(ty, ghost)` at once (current
    /// mutable): the kernels of history-dependent laws read the previous
    /// state while writing the current one.
    pub fn current_and_previous_mut(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<(ArrayMut<'_, T>, &Array<T>)> {
        self.materialize(ty, ghost);
        let name = &self.name;
        let prev = self
            .previous
            .as_ref()
            .ok_or_else(|| Error::invalid(format!("internal field '{name}' has no history")))?
            .values
            .get(ty, ghost)?;
        let cur = self.values.get_mut(ty, ghost)?;
        Ok((cur, prev))
    }

    /// All current values (exportable blocks).
    #[must_use]
    pub fn values(&self) -> &SharedElementTypeMapArray<T> {
        &self.values
    }

    /// All current values, mutable (performs every deferred save first;
    /// the Python bindings export blocks through it).
    pub fn values_mut(&mut self) -> &mut SharedElementTypeMapArray<T> {
        self.materialize_all();
        &mut self.values
    }

    /// Perform the deferred save of `(ty, ghost)`, if any.
    fn materialize(&mut self, ty: ElementType, ghost: GhostType) {
        if let Some(p) = &mut self.previous {
            p.materialize(&self.values, ty, ghost);
        }
    }

    /// Perform every deferred save.
    fn materialize_all(&mut self) {
        if let Some(p) = &mut self.previous {
            p.materialize_all(&self.values);
        }
    }

    /// `previous ← current` (upstream `saveCurrentValues`). A block whose
    /// current values are not exported and whose previous block has the
    /// same shape is saved lazily (module documentation, "Deferred
    /// saves"); the others are copied now.
    pub fn save_current_values(&mut self) -> Result<()> {
        let Some(p) = &mut self.previous else {
            return Err(Error::invalid(format!(
                "internal field '{}' has no history",
                self.name
            )));
        };
        for (ty, ghost, a) in self.values.as_map().iter_all() {
            if p.is_pending(ty, ghost) {
                // unchanged since that deferred save: the same save again
                continue;
            }
            let same_shape = p
                .values
                .as_map()
                .get(ty, ghost)
                .is_some_and(|d| d.len() == a.len() && d.nb_component() == a.nb_component());
            if same_shape && a.as_unexported().is_some() {
                p.pending.push((ty, ghost));
                continue;
            }
            save_block(&mut p.values, a, ty, ghost)?;
        }
        Ok(())
    }

    /// `current ← previous` (upstream `restorePreviousValues`); a block with
    /// a deferred save already holds its previous values.
    pub fn restore_previous_values(&mut self) -> Result<()> {
        match &self.previous {
            Some(p) => restore_blocks(&mut self.values, &p.values, &p.pending),
            None => Err(Error::invalid(format!(
                "internal field '{}' has no history",
                self.name
            ))),
        }
    }

    /// Compact the blocks after elements were removed from the law
    /// (upstream `removeIntegrationPoints`): `new_numbering(type, ghost)`
    /// maps every **law-local** element index to its new local index or
    /// `-1` (removed); `nb_quad(type, ghost)` is the number of quadrature
    /// points per element. Blocks absent from the field or empty are left
    /// alone. The history is compacted the same way (upstream does not —
    /// see the module documentation).
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] if a block does not have
    /// `renumbering.len() * nb_quad` tuples (upstream debug assertion);
    /// [`Error::InvalidIndex`] unless the new indices of the kept elements
    /// are exactly `0..kept` (each once, in any order; duplicates, gaps and
    /// indices past `kept` are rejected). Nothing is modified on error.
    pub fn remove_integration_points(
        &mut self,
        new_numbering: &ElementTypeMapArray<Idx>,
        nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
    ) -> Result<()> {
        self.materialize_all();
        let nbc = self.nb_component;
        let mut plans = Vec::new();
        for (ty, g, renumbering) in new_numbering.as_map().iter_all() {
            let Some(a) = self.values.as_map().get(ty, g) else {
                continue;
            };
            if a.is_empty() {
                continue;
            }
            let nq = nb_quad(ty, g);
            let plan = compaction_plan(renumbering.as_slice(), nq, a.len(), ty, g)?;
            plans.push((ty, g, nq, plan));
        }
        for (ty, g, nq, plan) in &plans {
            if let Some(a) = self.values.as_map_mut().get_mut(*ty, *g) {
                let mut a = a.write()?;
                let c = compacted(&a, plan, *nq, nbc);
                a.copy_from(&c)?;
            }
            if let Some(p) = &mut self.previous
                && let Some(a) = p.values.as_map_mut().get_mut(*ty, *g)
                && a.len() == plan.len() * nq
            {
                let c = compacted(a, plan, *nq, nbc);
                a.copy_from(&c)?;
            }
        }
        Ok(())
    }

    /// Upstream `flattenInternal` for one ghost type: scatter the field into
    /// mesh-indexed blocks (`nb_mesh_elements(type) * nb_quad` tuples, the
    /// filtered elements at their mesh positions). Blocks of `out` are
    /// allocated (with `fill`) when missing; existing blocks are resized to
    /// the mesh size, keeping their other values.
    ///
    /// # Errors
    /// A filter index outside the mesh block, or a size inconsistency.
    pub fn flatten(
        &self,
        filter: &ElementTypeMapArray<Idx>,
        ghost: GhostType,
        nb_mesh_elements: &dyn Fn(ElementType) -> usize,
        nb_quad: &dyn Fn(ElementType) -> usize,
        fill: T,
        out: &mut ElementTypeMapArray<T>,
    ) -> Result<()> {
        for (ty, src) in self.values.as_map().iter(ghost) {
            let Ok(f) = filter.get(ty, ghost) else {
                continue;
            };
            let src = src.read()?;
            let nq = nb_quad(ty);
            let n_dst = nb_mesh_elements(ty);
            let dst = out.alloc(
                akantu_core::checked_len(n_dst, nq, "flattened internal")?,
                self.nb_component,
                ty,
                ghost,
                fill.clone(),
            )?;
            if f.is_empty() {
                continue;
            }
            let chunk = nq * self.nb_component;
            if src.as_slice().len() != f.len() * chunk {
                return Err(Error::DimensionMismatch {
                    context: "InternalField::flatten",
                    expected: f.len() * chunk,
                    got: src.as_slice().len(),
                });
            }
            for (k, &el) in f.as_slice().iter().enumerate() {
                let e = akantu_core::try_to_usize(el)?;
                if e >= n_dst {
                    return Err(Error::InvalidIndex {
                        what: "element filter",
                        value: i64::from(el),
                    });
                }
                dst.as_mut_slice()[e * chunk..(e + 1) * chunk]
                    .clone_from_slice(&src.as_slice()[k * chunk..(k + 1) * chunk]);
            }
        }
        Ok(())
    }

    /// Upstream `inflateInternal` for one ghost type: gather mesh-indexed
    /// values back into the field (blocks of the field that do not exist
    /// are skipped, as upstream).
    ///
    /// # Errors
    /// Size or index inconsistencies (upstream debug assertions).
    pub fn inflate(
        &mut self,
        filter: &ElementTypeMapArray<Idx>,
        ghost: GhostType,
        nb_quad: &dyn Fn(ElementType) -> usize,
        field: &ElementTypeMapArray<T>,
    ) -> Result<()> {
        self.materialize_all();
        let nbc = self.nb_component;
        for (ty, src) in field.as_map().iter(ghost) {
            let Some(dest) = self.values.as_map_mut().get_mut(ty, ghost) else {
                continue;
            };
            let mut dest = dest.write()?;
            if src.nb_component() != nbc {
                return Err(Error::DimensionMismatch {
                    context: "InternalField::inflate",
                    expected: nbc,
                    got: src.nb_component(),
                });
            }
            let f = filter.get(ty, ghost)?;
            let chunk = nb_quad(ty) * nbc;
            if dest.as_slice().len() != f.len() * chunk {
                return Err(Error::DimensionMismatch {
                    context: "InternalField::inflate",
                    expected: f.len() * chunk,
                    got: dest.as_slice().len(),
                });
            }
            for (k, &el) in f.as_slice().iter().enumerate() {
                let e = akantu_core::try_to_usize(el)?;
                let s =
                    src.as_slice()
                        .get(e * chunk..(e + 1) * chunk)
                        .ok_or(Error::InvalidIndex {
                            what: "element filter",
                            value: i64::from(el),
                        })?;
                dest.as_mut_slice()[k * chunk..(k + 1) * chunk].clone_from_slice(s);
            }
        }
        Ok(())
    }
}

impl InternalField<Real> {
    /// New random field (upstream `registerInternal<Real,
    /// RandomInternalField>(id, nb_component)`): values are drawn from
    /// `param` (initially `RandomParameter{0}` = base 0, `uniform [0, 0]`,
    /// which still consumes engine calls, D-006).
    pub fn new_random(
        name: impl Into<Id>,
        nb_component: usize,
        param: RandomParameter,
    ) -> Result<Self> {
        let mut f = Self::new(name, nb_component, 0.0)?;
        f.filler = Filler::Random {
            param: Box::new(param),
            fill: random_fill,
            set_base: random_set_base,
        };
        Ok(f)
    }

    /// The random parameter of a random field.
    #[must_use]
    pub fn random_parameter(&self) -> Option<&RandomParameter> {
        match &self.filler {
            Filler::Random { param, .. } => Some(param),
            Filler::Default => None,
        }
    }

    /// Upstream `RandomInternalField::setRandomDistribution`: replace the
    /// parameter (with its distribution state, D-006) and redraw every
    /// value. On a plain field this turns it into a random field.
    pub fn set_random_distribution(&mut self, param: RandomParameter) {
        self.filler = Filler::Random {
            param: Box::new(param),
            fill: random_fill,
            set_base: random_set_base,
        };
        self.reset();
    }

    /// Value of the field as a parameter (upstream `operator Real()`): the
    /// default value, or the base value of a random field.
    #[must_use]
    pub fn parameter_value(&self) -> Real {
        match &self.filler {
            Filler::Random { param, .. } => param.base_value(),
            Filler::Default => self.default_value,
        }
    }
}

/// Validated compaction of one block: `plan[old] = Some(new)`.
fn compaction_plan(
    renumbering: &[Idx],
    nq: usize,
    len: usize,
    ty: ElementType,
    ghost: GhostType,
) -> Result<Vec<Option<usize>>> {
    if renumbering.len() * nq != len {
        return Err(Error::DimensionMismatch {
            context: "InternalField::remove_integration_points",
            expected: renumbering.len() * nq,
            got: len,
        });
    }
    let plan: Vec<Option<usize>> = renumbering
        .iter()
        .map(|&n| akantu_core::decode_idx(n))
        .collect::<Result<_>>()?;
    // The kept elements must be renumbered onto `[0, kept)` exactly once
    // each: a destination outside that range (a gap)
    // or a repeated one would truncate to `kept` entries with stale or
    // missing data.
    let kept = plan.iter().flatten().count();
    let mut seen = vec![false; kept];
    for &n in plan.iter().flatten() {
        let value = i64::try_from(n).unwrap_or(i64::MAX);
        match seen.get_mut(n) {
            None => {
                return Err(Error::InvalidIndex {
                    what: "law-local renumbering (destination past the number of kept elements)",
                    value,
                });
            }
            Some(true) => {
                return Err(Error::InvalidIndex {
                    what: "law-local renumbering (duplicate destination)",
                    value,
                });
            }
            Some(s) => *s = true,
        }
    }
    let _ = (ty, ghost);
    Ok(plan)
}

/// Upstream: copy kept elements to their new positions in a temporary of
/// the old size, truncate to the number of kept elements (the caller copies
/// it back with `copy_from`).
fn compacted<T: Clone>(a: &Array<T>, plan: &[Option<usize>], nq: usize, nbc: usize) -> Array<T> {
    let chunk = nq * nbc;
    let mut tmp = a.clone();
    let mut kept = 0;
    for (old, new) in plan.iter().enumerate() {
        if let Some(new) = new {
            tmp.as_mut_slice()[new * chunk..(new + 1) * chunk]
                .clone_from_slice(&a.as_slice()[old * chunk..(old + 1) * chunk]);
            kept += 1;
        }
    }
    tmp.truncate(kept * nq);
    tmp
}

/// Old block lengths of a field, for callers that need upstream's
/// `resize()` bookkeeping (old size per `(type, ghost)`).
#[must_use]
pub fn block_lengths<T>(values: &SharedElementTypeMapArray<T>) -> ElementTypeMap<usize> {
    let mut m = ElementTypeMap::new();
    for (ty, g, a) in values.as_map().iter_all() {
        let _ = m.insert(ty, g, a.len());
    }
    m
}

/// Internal field of any supported value type, owned (storage of
/// runtime-registered internals).
#[derive(Clone, Debug)]
pub enum AnyInternalField {
    /// `Real` field.
    Real(InternalField<akantu_core::Real>),
    /// `Int` field.
    Int(InternalField<akantu_core::Int>),
    /// `UInt` field.
    UInt(InternalField<akantu_core::UInt>),
    /// `bool` field.
    Bool(InternalField<bool>),
}

impl AnyInternalField {
    /// Reflection view.
    #[must_use]
    pub fn as_ref(&self) -> crate::constitutive_law::InternalFieldRef<'_> {
        use crate::constitutive_law::InternalFieldRef as R;
        match self {
            Self::Real(f) => R::Real(f),
            Self::Int(f) => R::Int(f),
            Self::UInt(f) => R::UInt(f),
            Self::Bool(f) => R::Bool(f),
        }
    }

    /// Mutable reflection view.
    pub fn as_mut(&mut self) -> crate::constitutive_law::InternalFieldMut<'_> {
        use crate::constitutive_law::InternalFieldMut as M;
        match self {
            Self::Real(f) => M::Real(f),
            Self::Int(f) => M::Int(f),
            Self::UInt(f) => M::UInt(f),
            Self::Bool(f) => M::Bool(f),
        }
    }

    /// Resize block `(ty, ghost)` to `n` quadrature points.
    pub fn resize(&mut self, ty: ElementType, ghost: GhostType, n: usize) -> Result<()> {
        match self {
            Self::Real(f) => f.resize(ty, ghost, n),
            Self::Int(f) => f.resize(ty, ghost, n),
            Self::UInt(f) => f.resize(ty, ghost, n),
            Self::Bool(f) => f.resize(ty, ghost, n),
        }
    }
}

/// Law-owned, runtime-registered internal fields in registration order
/// (upstream `registerInternal<T>(name, nb_component)` called from Python,
/// `ConstitutiveLawInternalHandler`). Same history/resize/reflection
/// contract as typed members.
#[derive(Clone, Debug, Default)]
pub struct DynamicInternals {
    fields: Vec<(Id, AnyInternalField)>,
}

impl DynamicInternals {
    /// Register a field (error if the name exists).
    pub fn register(&mut self, name: &str, field: AnyInternalField) -> Result<()> {
        if self.fields.iter().any(|(n, _)| n == name) {
            return Err(Error::AlreadyExists {
                kind: "internal field",
                id: name.into(),
            });
        }
        self.fields.push((name.into(), field));
        Ok(())
    }

    /// Register a `Real` field (Python `registerInternalReal`).
    pub fn register_real(&mut self, name: &str, nb_component: usize) -> Result<()> {
        self.register(
            name,
            AnyInternalField::Real(InternalField::new(name, nb_component, 0.0)?),
        )
    }

    /// Register a `Real` field on the model's FE engine `fe_engine_id`,
    /// sized on `element_filter` or, when `None`, on the law's filter
    /// (upstream `registerInternal<Real>(id, nb_component, fe_engine_id[,
    /// element_filter])`, CL-001; [`InternalField::set_fe_engine_id`]).
    pub fn register_real_on(
        &mut self,
        name: &str,
        nb_component: usize,
        fe_engine_id: &str,
        element_filter: Option<ElementTypeMapArray<Idx>>,
    ) -> Result<()> {
        let mut f = InternalField::new(name, nb_component, 0.0)?;
        f.set_fe_engine_id(fe_engine_id);
        if let Some(filter) = element_filter {
            f.set_element_filter(filter);
        }
        self.register(name, AnyInternalField::Real(f))
    }

    /// Register an `Int` field (Python `registerInternalInt`).
    pub fn register_int(&mut self, name: &str, nb_component: usize) -> Result<()> {
        self.register(
            name,
            AnyInternalField::Int(InternalField::new(name, nb_component, 0)?),
        )
    }

    /// Field by name.
    pub fn get(&self, name: &str) -> Result<&AnyInternalField> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, f)| f)
            .ok_or_else(|| Error::not_found("internal field", name))
    }

    /// Mutable field by name.
    pub fn get_mut(&mut self, name: &str) -> Result<&mut AnyInternalField> {
        self.fields
            .iter_mut()
            .find(|(n, _)| n == name)
            .map(|(_, f)| f)
            .ok_or_else(|| Error::not_found("internal field", name))
    }

    /// `Real` field by name (Python `getInternalReal`).
    pub fn real(&self, name: &str) -> Result<&InternalField<akantu_core::Real>> {
        match self.get(name)? {
            AnyInternalField::Real(f) => Ok(f),
            _ => Err(Error::invalid(format!(
                "internal field '{name}' is not Real"
            ))),
        }
    }

    /// Mutable `Real` field by name.
    pub fn real_mut(&mut self, name: &str) -> Result<&mut InternalField<akantu_core::Real>> {
        match self.get_mut(name)? {
            AnyInternalField::Real(f) => Ok(f),
            _ => Err(Error::invalid(format!(
                "internal field '{name}' is not Real"
            ))),
        }
    }

    /// Iterate in registration order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &AnyInternalField)> {
        self.fields.iter().map(|(n, f)| (n.as_str(), f))
    }

    /// Iterate mutably in registration order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut AnyInternalField)> {
        self.fields.iter_mut().map(|(n, f)| (n.as_str(), f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::random::{IdxSlot, MinstdRand0, RandomDistributionType, RandomGenerator};

    const TRI: ElementType = ElementType::Triangle3;
    const NG: GhostType = GhostType::NotGhost;

    /// CL-002: a restricted field only allocates, resizes and
    /// resets blocks of its dimension and kind, whatever else the filter
    /// holds (facets, cohesive elements, empty blocks).
    #[test]
    fn restriction_selects_the_blocks_of_dimension_and_kind() {
        let _guard = ENGINE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut filter = ElementTypeMapArray::<Idx>::new("filter");
        let seg = ElementType::Segment2;
        let coh = ElementType::Cohesive2d4;
        let quad = ElementType::Quadrangle4;
        filter
            .as_map_mut()
            .insert(TRI, NG, Array::from_vec(vec![3, 1], 1).unwrap())
            .unwrap();
        filter
            .as_map_mut()
            .insert(seg, NG, Array::from_vec(vec![0], 1).unwrap())
            .unwrap();
        filter
            .as_map_mut()
            .insert(coh, NG, Array::from_vec(vec![0, 1, 2], 1).unwrap())
            .unwrap();
        filter
            .as_map_mut()
            .insert(quad, GhostType::Ghost, Array::from_vec(vec![], 1).unwrap())
            .unwrap();
        let nq = |ty: ElementType, _: GhostType| if ty == quad { 4 } else { 2 };
        let mut regular: InternalField<f64> = InternalField::new("r", 2, 7.0).unwrap();
        regular.set_restriction(2, ElementKind::Regular);
        assert!(regular.handles(TRI) && regular.handles(quad));
        assert!(!regular.handles(seg) && !regular.handles(coh));
        regular.resize_from_filter(&filter, &nq).unwrap();
        assert_eq!(regular.get(TRI, NG).unwrap().len(), 4);
        assert!(regular.get(quad, GhostType::Ghost).unwrap().is_empty());
        assert!(!regular.exists(seg, NG) && !regular.exists(coh, NG));
        assert!(
            regular
                .get(TRI, NG)
                .unwrap()
                .as_slice()
                .iter()
                .all(|&v| v == 7.0)
        );

        // a cohesive random field on the same filter: only the cohesive block,
        // and the draws consume exactly its values
        RandomGenerator::<IdxSlot>::seed(11);
        let p = RandomParameter::make(0.0, RandomDistributionType::Uniform, 0.0, 1.0);
        let mut cohesive = InternalField::new_random("c", 1, p.clone()).unwrap();
        cohesive.set_restriction(2, ElementKind::Cohesive);
        cohesive.resize_from_filter(&filter, &nq).unwrap();
        assert!(!cohesive.exists(TRI, NG));
        let mut e = MinstdRand0::with_seed(11);
        let mut q = p.clone();
        let mut expect = vec![0.0; 6];
        q.set_values_with(&mut e, &mut expect);
        assert_eq!(cohesive.get(coh, NG).unwrap().as_slice(), &expect[..]);

        // upstream setElementKind: a regular random field switched to another
        // kind no longer updates its regular blocks (reset and resize skip them)
        let mut switched = InternalField::new_random("s", 1, p).unwrap();
        switched.set_restriction(2, ElementKind::Regular);
        switched.resize_from_filter(&filter, &nq).unwrap();
        let before = switched.get(TRI, NG).unwrap().as_slice().to_vec();
        switched.set_element_kind(ElementKind::Structural);
        assert_eq!(switched.restriction().unwrap().spatial_dimension, Some(2));
        let engine = RandomGenerator::<IdxSlot>::engine_snapshot();
        switched.reset();
        filter
            .get_mut(TRI, NG)
            .unwrap()
            .try_push_tuple(&[0])
            .unwrap();
        switched.resize_from_filter(&filter, &nq).unwrap();
        assert_eq!(switched.get(TRI, NG).unwrap().as_slice(), &before[..]);
        assert_eq!(
            format!("{:?}", RandomGenerator::<IdxSlot>::engine_snapshot()),
            format!("{engine:?}"),
            "no draw"
        );
        // unrestricted fields keep the explicit-block behaviour
        let mut plain: InternalField<f64> = InternalField::new("p", 1, 1.0).unwrap();
        assert!(plain.restriction().is_none() && plain.handles(seg));
        plain.resize_from_filter(&filter, &nq).unwrap();
        assert!(plain.exists(seg, NG) && plain.exists(coh, NG));
    }

    /// The explicit-block resizes of a restricted field
    /// (the production path of every law) skip the types it does not
    /// handle — no block, no history block, no draw — and
    /// `adopt_restriction` gives unrestricted fields the law's default.
    #[test]
    fn explicit_block_resizes_honour_the_restriction() {
        let _guard = ENGINE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let seg = ElementType::Segment2;
        let coh = ElementType::Cohesive2d4;
        let law = TypeRestriction {
            spatial_dimension: Some(2),
            element_kind: ElementKind::Regular,
        };
        let blocks = [
            (coh, NG, 3),
            (TRI, GhostType::Ghost, 1),
            (seg, NG, 2),
            (TRI, NG, 2),
        ];
        RandomGenerator::<IdxSlot>::seed(5);
        let p = RandomParameter::make(1.0, RandomDistributionType::Uniform, 0.0, 2.0);
        let mut f = InternalField::new_random("f", 1, p.clone()).unwrap();
        f.initialize_history();
        f.adopt_restriction(law);
        assert_eq!(f.restriction(), Some(law));
        f.resize_blocks(&blocks).unwrap();
        f.resize(seg, GhostType::Ghost, 4).unwrap();
        for ty in [seg, coh] {
            for g in GhostType::ALL {
                assert!(!f.exists(ty, g) && f.previous(ty, g).is_err(), "{ty} {g:?}");
            }
        }
        // draws: not-ghost triangles, then the ghost triangle, nothing else
        let mut e = MinstdRand0::with_seed(5);
        let mut q = p;
        let mut expect = [0.0; 3];
        q.set_values_with(&mut e, &mut expect);
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &expect[..2]);
        assert_eq!(
            f.get(TRI, GhostType::Ghost).unwrap().as_slice(),
            &expect[2..]
        );
        assert_eq!(f.previous(TRI, NG).unwrap().len(), 2);
        assert_eq!(
            RandomGenerator::<IdxSlot>::new().next_u32(),
            e.next_u32(),
            "no draw for the excluded blocks"
        );

        // adoption keeps an explicit restriction, completes a kind-only one
        let mut own: InternalField<f64> = InternalField::new("own", 1, 0.0).unwrap();
        own.set_restriction(3, ElementKind::Regular);
        own.adopt_restriction(law);
        assert_eq!(own.restriction().unwrap().spatial_dimension, Some(3));
        let mut kind: InternalField<f64> = InternalField::new("kind", 1, 0.0).unwrap();
        kind.set_element_kind(ElementKind::Cohesive);
        assert_eq!(kind.restriction().unwrap().spatial_dimension, None);
        kind.adopt_restriction(law);
        assert_eq!(
            kind.restriction(),
            Some(TypeRestriction {
                spatial_dimension: Some(2),
                element_kind: ElementKind::Cohesive
            })
        );
        kind.resize_blocks(&blocks).unwrap();
        assert!(kind.exists(coh, NG) && !kind.exists(TRI, NG) && !kind.exists(seg, NG));
    }

    #[test]
    fn history_roundtrip() {
        let mut f: InternalField<f64> = InternalField::new("damage", 1, 0.0).unwrap();
        assert!(InternalField::<f64>::new("x", 0, 0.0).is_err());
        f.initialize_history();
        f.resize(TRI, NG, 3).unwrap();
        f.get_mut(TRI, NG).unwrap().as_mut_slice()[1] = 0.5;
        f.save_current_values().unwrap();
        f.get_mut(TRI, NG).unwrap().as_mut_slice()[1] = 0.9;
        assert_eq!(f.previous(TRI, NG).unwrap().as_slice()[1], 0.5);
        f.restore_previous_values().unwrap();
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[0.0, 0.5, 0.0]);
        let mut g: InternalField<f64> = InternalField::new("stress", 4, 0.0).unwrap();
        assert!(g.save_current_values().is_err());
        g.resize(ElementType::Quadrangle4, GhostType::Ghost, 2)
            .unwrap();
        assert_eq!(
            g.get(ElementType::Quadrangle4, GhostType::Ghost)
                .unwrap()
                .len(),
            2
        );
        assert!(
            g.previous(ElementType::Quadrangle4, GhostType::Ghost)
                .is_err()
        );
    }

    /// Upstream `setDefaultValue` resets current values (not the history)
    /// and is the value of new tuples; the history keeps its own default.
    #[test]
    fn default_value_semantics() {
        let mut f: InternalField<f64> = InternalField::new("delta_T", 1, 0.0).unwrap();
        f.resize(TRI, NG, 2).unwrap();
        f.get_mut(TRI, NG).unwrap().as_mut_slice()[0] = 7.0;
        f.initialize_history();
        // History created with the default, not a copy of current values.
        assert_eq!(f.previous(TRI, NG).unwrap().as_slice(), &[0.0, 0.0]);
        f.set_default_value(3.0);
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[3.0, 3.0]);
        f.resize(TRI, NG, 3).unwrap();
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[3.0, 3.0, 3.0]);
        assert_eq!(f.previous(TRI, NG).unwrap().as_slice(), &[0.0, 0.0, 0.0]);
        f.get_mut(TRI, NG).unwrap().as_mut_slice()[2] = 1.0;
        f.resize(TRI, NG, 2).unwrap();
        f.resize(TRI, NG, 3).unwrap();
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[3.0, 3.0, 3.0]);
    }

    /// D-006: random fields draw from the global engine in upstream order —
    /// whole field on reset (ghost type, then enum order), only the new tail
    /// on resize; the history never draws.
    #[test]
    fn random_field_draw_order() {
        // Serialise with other users of the global engine in this binary.
        let _guard = ENGINE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        RandomGenerator::<IdxSlot>::seed(42);
        let p = RandomParameter::make(1.0, RandomDistributionType::Uniform, 0.0, 2.0);
        let mut f = InternalField::new_random("Yd", 2, p.clone()).unwrap();
        f.initialize_history();
        // Blocks given out of order: drawn NotGhost first, enum order.
        f.resize_blocks(&[
            (ElementType::Triangle6, NG, 1),
            (TRI, GhostType::Ghost, 1),
            (TRI, NG, 2),
        ])
        .unwrap();
        let mut e = MinstdRand0::with_seed(42);
        let mut q = p.clone();
        let mut expect = vec![0.0; 4 + 2 + 2];
        q.set_values_with(&mut e, &mut expect);
        let got: Vec<f64> = [
            (TRI, NG),
            (ElementType::Triangle6, NG),
            (TRI, GhostType::Ghost),
        ]
        .iter()
        .flat_map(|&(t, g)| f.get(t, g).unwrap().as_slice().to_vec())
        .collect();
        assert_eq!(got, expect);
        assert!(
            f.previous(TRI, NG)
                .unwrap()
                .as_slice()
                .iter()
                .all(|&v| v == 0.0)
        );
        // Resize: only the new tail draws.
        f.resize(TRI, NG, 3).unwrap();
        let mut tail = [0.0; 2];
        q.set_values_with(&mut e, &mut tail);
        assert_eq!(&f.get(TRI, NG).unwrap().as_slice()[4..], &tail);
        assert_eq!(&f.get(TRI, NG).unwrap().as_slice()[..4], &expect[..4]);
        // setDefaultValue: base value, whole field redrawn.
        f.set_default_value(10.0);
        assert_eq!(f.parameter_value(), 10.0);
        let mut q10 = q.clone();
        q10.set_base_value(10.0);
        let mut all = vec![0.0; 6 + 2 + 2];
        q10.set_values_with(&mut e, &mut all);
        let got: Vec<f64> = [
            (TRI, NG),
            (ElementType::Triangle6, NG),
            (TRI, GhostType::Ghost),
        ]
        .iter()
        .flat_map(|&(t, g)| f.get(t, g).unwrap().as_slice().to_vec())
        .collect();
        assert_eq!(got, all);
        assert_eq!(
            RandomGenerator::<IdxSlot>::engine_snapshot(),
            e,
            "engine advanced exactly by the draws"
        );
    }

    pub(crate) static ENGINE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn remove_integration_points_compacts_values_and_history() {
        let mut f: InternalField<f64> = InternalField::new("s", 2, 0.0).unwrap();
        f.initialize_history();
        f.resize(TRI, NG, 3 * 2).unwrap(); // 3 elements, 2 quad points
        for (i, v) in f
            .get_mut(TRI, NG)
            .unwrap()
            .as_mut_slice()
            .iter_mut()
            .enumerate()
        {
            *v = i as f64;
        }
        f.save_current_values().unwrap();
        let mut num = ElementTypeMapArray::<Idx>::new("n");
        num.as_map_mut()
            .insert(TRI, NG, Array::from_vec(vec![1, -1, 0], 1).unwrap())
            .unwrap();
        f.remove_integration_points(&num, &|_, _| 2).unwrap();
        // element 2 -> 0, element 0 -> 1, element 1 removed
        let expect = [8.0, 9.0, 10.0, 11.0, 0.0, 1.0, 2.0, 3.0];
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &expect);
        assert_eq!(f.previous(TRI, NG).unwrap().as_slice(), &expect);
        // inconsistent numbering: nothing changes
        let mut bad = ElementTypeMapArray::<Idx>::new("n");
        bad.as_map_mut()
            .insert(TRI, NG, Array::from_vec(vec![0, 1, 2], 1).unwrap())
            .unwrap();
        assert!(f.remove_integration_points(&bad, &|_, _| 2).is_err());
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &expect);
    }

    /// The renumbering must map the kept elements onto
    /// `0..kept` exactly once each. The numbering `[2, -1, -1]` (one kept
    /// element sent to index 2) and a duplicate destination `[0, 0, -1]`
    /// used to return `Ok` with a truncated, wrong block; both are rejected
    /// before anything (current or history) is modified. A permutation of
    /// `0..kept` is still accepted.
    #[test]
    fn remove_integration_points_rejects_gaps_and_duplicates() {
        let numbering = |v: Vec<Idx>| {
            let mut n = ElementTypeMapArray::<Idx>::new("n");
            n.as_map_mut()
                .insert(TRI, NG, Array::from_vec(v, 1).unwrap())
                .unwrap();
            n
        };
        let mut f: InternalField<f64> = InternalField::new("s", 1, 0.0).unwrap();
        f.initialize_history();
        f.resize(TRI, NG, 3).unwrap();
        f.get_mut(TRI, NG)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&[10.0, 20.0, 30.0]);
        f.save_current_values().unwrap();
        for bad in [
            vec![2, -1, -1],
            vec![0, 0, -1],
            vec![1, -1, 1],
            vec![0, 2, -1],
        ] {
            let err = f
                .remove_integration_points(&numbering(bad.clone()), &|_, _| 1)
                .unwrap_err();
            assert!(
                matches!(err, Error::InvalidIndex { .. }),
                "{bad:?}: {err:?}"
            );
            assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[10.0, 20.0, 30.0]);
            assert_eq!(f.previous(TRI, NG).unwrap().as_slice(), &[10.0, 20.0, 30.0]);
        }
        // a valid permutation of 0..kept: element 2 -> 0, element 1 -> 1
        f.remove_integration_points(&numbering(vec![-1, 1, 0]), &|_, _| 1)
            .unwrap();
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[30.0, 20.0]);
        assert_eq!(f.previous(TRI, NG).unwrap().as_slice(), &[30.0, 20.0]);
        // removing everything is valid
        f.remove_integration_points(&numbering(vec![-1, -1]), &|_, _| 1)
            .unwrap();
        assert!(f.get(TRI, NG).unwrap().is_empty());
    }

    #[test]
    fn flatten_inflate_roundtrip() {
        let mut f: InternalField<f64> = InternalField::new("d", 1, 0.0).unwrap();
        f.resize(TRI, NG, 2).unwrap();
        f.get_mut(TRI, NG)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&[5.0, 6.0]);
        let mut filter = ElementTypeMapArray::<Idx>::new("filter");
        filter
            .as_map_mut()
            .insert(TRI, NG, Array::from_vec(vec![3, 1], 1).unwrap())
            .unwrap();
        let mut flat = ElementTypeMapArray::new("flat");
        f.flatten(&filter, NG, &|_| 4, &|_| 1, -1.0, &mut flat)
            .unwrap();
        assert_eq!(
            flat.get(TRI, NG).unwrap().as_slice(),
            &[-1.0, 6.0, -1.0, 5.0]
        );
        flat.get_mut(TRI, NG).unwrap().as_mut_slice()[1] = 60.0;
        f.inflate(&filter, NG, &|_| 1, &flat).unwrap();
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[5.0, 60.0]);
    }

    /// Deferred saves: every reader sees the values of an immediate copy,
    /// whatever the order of saves, restores, writes, announced rewrites,
    /// commits, resizes and resets (a random sequence against two plain
    /// vectors that copy at every save).
    #[test]
    fn deferred_saves_match_immediate_copies() {
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move |m: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % m
        };
        for run in 0..200 {
            let nbc = 1 + run % 3;
            let mut f: InternalField<f64> = InternalField::new("s", nbc, 0.25).unwrap();
            f.resize(TRI, NG, 4).unwrap();
            f.initialize_history();
            let mut cur = vec![0.25; 4 * nbc];
            let mut prev = cur.clone();
            let mut stamp = 1.0;
            for _ in 0..60 {
                stamp += 1.0;
                match next(9) {
                    0 | 1 => {
                        f.save_current_values().unwrap();
                        prev.clone_from(&cur);
                    }
                    2 => {
                        f.restore_previous_values().unwrap();
                        cur.clone_from(&prev);
                    }
                    3 => {
                        let i = usize::try_from(next(cur.len() as u64)).unwrap();
                        f.get_mut(TRI, NG).unwrap().as_mut_slice()[i] = stamp;
                        cur[i] = stamp;
                    }
                    4 => {
                        f.prepare_overwrite(TRI, NG);
                        let mut g = f.get_mut(TRI, NG).unwrap();
                        for (k, v) in g.as_mut_slice().iter_mut().enumerate() {
                            *v = stamp + k as f64;
                        }
                        for (k, v) in cur.iter_mut().enumerate() {
                            *v = stamp + k as f64;
                        }
                    }
                    5 => {
                        let n = cur.len() / nbc;
                        let new: Vec<f64> = (0..cur.len()).map(|k| -stamp - k as f64).collect();
                        let mut a = Array::from_vec(new.clone(), nbc).unwrap();
                        assert_eq!(a.len(), n);
                        f.commit_overwrite(TRI, NG, &mut a).unwrap();
                        cur = new;
                    }
                    6 => {
                        let n = 2 + usize::try_from(next(5)).unwrap();
                        f.resize(TRI, NG, n).unwrap();
                        cur.resize(n * nbc, 0.25);
                        prev.resize(n * nbc, 0.25);
                    }
                    7 => {
                        f.reset();
                        cur.iter_mut().for_each(|v| *v = 0.25);
                    }
                    _ => {
                        let (c, p) = f.current_and_previous_mut(TRI, NG).unwrap();
                        assert_eq!(c.as_slice(), cur.as_slice());
                        assert_eq!(p.as_slice(), prev.as_slice());
                    }
                }
                assert_eq!(f.get(TRI, NG).unwrap().as_slice(), cur.as_slice());
                assert_eq!(f.previous(TRI, NG).unwrap().as_slice(), prev.as_slice());
                let c = f.clone();
                assert_eq!(c.previous(TRI, NG).unwrap().as_slice(), prev.as_slice());
            }
        }
    }

    /// A save defers the copy; an announced rewrite exchanges the
    /// allocations instead of copying; a commit leaves the old values in
    /// the caller's array; an unchanged block is not copied again. (In the
    /// test-only export-all mode every block counts as exported: the saves
    /// are copies, with the same values.)
    #[test]
    fn deferred_save_and_overwrite() {
        let lazy = !akantu_core::array_shared::export_all::ENABLED;
        let deferred = if lazy { vec![(TRI, NG)] } else { Vec::new() };
        let mut f: InternalField<f64> = InternalField::new("stress", 2, 0.0).unwrap();
        f.resize(TRI, NG, 2).unwrap();
        f.initialize_history();
        f.get_mut(TRI, NG)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        f.save_current_values().unwrap();
        assert_eq!(f.previous.as_ref().unwrap().pending, deferred);
        assert_eq!(
            f.previous(TRI, NG).unwrap().as_slice(),
            &[1.0, 2.0, 3.0, 4.0]
        );
        f.save_current_values().unwrap();
        f.restore_previous_values().unwrap();
        assert_eq!(f.previous.as_ref().unwrap().pending, deferred);
        let before = f.get(TRI, NG).unwrap().as_slice().as_ptr();
        f.prepare_overwrite(TRI, NG);
        assert!(f.previous.as_ref().unwrap().pending.is_empty());
        if lazy {
            assert_eq!(
                f.previous(TRI, NG).unwrap().as_slice().as_ptr(),
                before,
                "the old allocation became the history"
            );
        }
        f.get_mut(TRI, NG)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&[5.0, 6.0, 7.0, 8.0]);
        assert_eq!(
            f.previous(TRI, NG).unwrap().as_slice(),
            &[1.0, 2.0, 3.0, 4.0]
        );
        // ids stay with their blocks
        assert!(
            f.previous(TRI, NG)
                .unwrap()
                .id()
                .starts_with("previous_stress")
        );
        f.save_current_values().unwrap();
        let mut new = Array::from_vec(vec![9.0, 10.0, 11.0, 12.0], 2).unwrap();
        f.commit_overwrite(TRI, NG, &mut new).unwrap();
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[9.0, 10.0, 11.0, 12.0]);
        assert_eq!(
            f.previous(TRI, NG).unwrap().as_slice(),
            &[5.0, 6.0, 7.0, 8.0]
        );
        let mut wrong = Array::from_vec(vec![0.0; 2], 2).unwrap();
        assert!(f.commit_overwrite(TRI, NG, &mut wrong).is_err());
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[9.0, 10.0, 11.0, 12.0]);
        f.restore_previous_values().unwrap();
        assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &[5.0, 6.0, 7.0, 8.0]);
    }

    /// An exported block (a NumPy view, D-047) is saved by copy at once and
    /// never exchanged: the view keeps its allocation and sees every value
    /// the field holds; after the view is gone the saves are deferred
    /// again. Exporting performs the deferred saves first.
    #[test]
    fn exported_blocks_are_copied_and_keep_their_views() {
        use akantu_core::array_shared::ForeignRuntime;
        let rt = ForeignRuntime::new();
        let mut f: InternalField<f64> = InternalField::new("stress", 1, 0.0).unwrap();
        f.resize(TRI, NG, 3).unwrap();
        f.initialize_history();
        f.get_mut(TRI, NG)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&[1.0, 2.0, 3.0]);
        f.save_current_values().unwrap(); // deferred
        let lease = f
            .values_mut()
            .shared_mut(TRI, NG)
            .unwrap()
            .export(&rt)
            .unwrap();
        assert!(
            f.previous.as_ref().unwrap().pending.is_empty(),
            "exporting saved"
        );
        let mut interp = rt.enter().unwrap();
        {
            let _s = interp.native().unwrap();
            assert_eq!(f.previous(TRI, NG).unwrap().as_slice(), &[1.0, 2.0, 3.0]);
            f.get_mut(TRI, NG).unwrap().as_mut_slice()[0] = 4.0;
            f.save_current_values().unwrap();
            assert!(
                f.previous.as_ref().unwrap().pending.is_empty(),
                "copied at once"
            );
            f.prepare_overwrite(TRI, NG);
            let mut new = Array::from_vec(vec![7.0, 8.0, 9.0], 1).unwrap();
            f.commit_overwrite(TRI, NG, &mut new).unwrap();
            assert_eq!(f.previous(TRI, NG).unwrap().as_slice(), &[4.0, 2.0, 3.0]);
            assert!(f.values().shared(TRI, NG).unwrap().is_attached(&lease));
            let cur = f.get(TRI, NG).unwrap();
            assert_eq!(cur.as_slice().as_ptr().cast_mut(), lease.as_ptr());
            assert_eq!(cur.as_slice(), &[7.0, 8.0, 9.0]);
        }
        lease.check_foreign_access(&interp).unwrap();
        drop(interp);
        drop(lease);
        f.save_current_values().unwrap();
        // deferred again (not in the test-only export-all mode, where every
        // block counts as exported)
        let lazy = !akantu_core::array_shared::export_all::ENABLED;
        let deferred = if lazy { vec![(TRI, NG)] } else { Vec::new() };
        assert_eq!(f.previous.as_ref().unwrap().pending, deferred);
    }
}
