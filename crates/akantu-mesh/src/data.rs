// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh_data.hh, mesh_data_tmpl.hh,
// mesh_data.cc, src/mesh/element_type_map_tmpl.hh (onElementsRemoved),
// src/mesh/mesh_inline_impl.hh (removeNodesFromArray).

//! Typed, named per-element and per-node data (upstream `MeshData`).
//!
//! Well-known names written by readers and algorithms: `tag_0`, `tag_1`
//! (Gmsh tags), `physical_names`, `element_to_subelement`,
//! `subelement_to_element`, `mesh_to_mesh_facet`, `facet_to_double`, …
//! Upstream stores `ElementTypeMapArray<T>` for a closed list of `T`; this is
//! the [`MeshDataValues`] enum. As upstream, the type of a dataset is fixed by
//! its first allocation and checked on every typed access
//! ([`MeshData::element_map_mut_or_insert`] and siblings return
//! `Error::Mesh` on a type mismatch instead of throwing).
//!
//! Element/node removal (`crate::mutation`):
//! upstream's `RemovedElementsEvent`/`RemovedNodesEvent` update the
//! connectivities and the node arrays only — mesh data keeps stale rows
//! (UD-117, category B; multi-tuple truncation UD-103). Here the rows of
//! every dataset follow their element/node (`MeshData::apply_element_numbering`,
//! `MeshData::apply_node_numbering`, crate-private) and element references
//! are remapped (`MeshData::remap_element_references`).

use std::collections::{BTreeMap, BTreeSet};

use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Int, Real, Result,
    SharedArray, SharedElementTypeMapArray, UInt, try_to_usize,
};

/// Element-wise data of one of the supported element types.
#[derive(Clone, Debug)]
///
/// The numeric datasets (`Int`, `UInt`, `Real`, `bool`) are exportable
/// (D-047): Python `getElementalData*` / `getDataReal` views alias them.
pub enum MeshDataValues {
    /// `Int` values (Gmsh tags…).
    Int(SharedElementTypeMapArray<Int>),
    /// `UInt`/`Idx` values.
    UInt(SharedElementTypeMapArray<UInt>),
    /// `Real` values.
    Real(SharedElementTypeMapArray<Real>),
    /// Strings (`physical_names`).
    String(ElementTypeMapArray<String>),
    /// Element references (`element_to_subelement`, `subelement_to_element`).
    Element(ElementTypeMapArray<Element>),
    /// Element lists per element (`element_to_subelement` for facets with a
    /// variable number of neighbours).
    ElementList(ElementTypeMapArray<Vec<Element>>),
    /// Booleans.
    Bool(SharedElementTypeMapArray<bool>),
}

/// Upstream `MeshDataTypeCode` (the order is upstream's enum order:
/// `_int, _uint, _real, _bool, _element, _std_string, _std_vector_element`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MeshDataTypeCode {
    /// `_int`.
    Int,
    /// `_uint`.
    UInt,
    /// `_real`.
    Real,
    /// `_bool`.
    Bool,
    /// `_element`.
    Element,
    /// `_std_string`.
    String,
    /// `_std_vector_element`.
    ElementList,
}

/// Apply `$body` to the `ElementTypeMapArray` inside any variant.
macro_rules! with_values {
    ($v:expr, $m:ident => $body:expr) => {
        match $v {
            MeshDataValues::Int($m) => $body,
            MeshDataValues::UInt($m) => $body,
            MeshDataValues::Real($m) => $body,
            MeshDataValues::String($m) => $body,
            MeshDataValues::Element($m) => $body,
            MeshDataValues::ElementList($m) => $body,
            MeshDataValues::Bool($m) => $body,
        }
    };
}

/// Keep the rows of `a` whose new index is not `-1`, moved to their new
/// index (upstream `ElementTypeMapArray::onElementsRemoved` /
/// `Mesh::removeNodesFromArray`). An array may hold several tuples per
/// entity (`len = k · numbering.len()`); all `k` tuples follow the entity
/// (upstream truncates such arrays to one tuple per entity — not
/// reproduced). An empty array is left alone; any other length mismatch is
/// an error. `numbering` must be a valid compaction (checked by the
/// callers: dense, injective new indices).
pub(crate) fn compact_rows<T: Clone>(a: &mut Array<T>, numbering: &[Idx]) -> Result<()> {
    if let Some(c) = compacted_rows(a, numbering)? {
        *a = c;
    }
    Ok(())
}

/// [`compact_rows`] on an exportable array: the compacted values replace
/// the block (an exported block is retired: its views go stale, ARCHITECTURE.md, "Shared storage and its aliasing rules").
pub(crate) fn compact_shared_rows<T: Clone>(
    a: &mut SharedArray<T>,
    numbering: &[Idx],
) -> Result<()> {
    let c = compacted_rows(&*a.read()?, numbering)?;
    if let Some(c) = c {
        a.write()?.replace(c);
    }
    Ok(())
}

/// The rows of `a` kept by `numbering`, in their new order (`None`: nothing
/// to compact).
fn compacted_rows<T: Clone>(a: &Array<T>, numbering: &[Idx]) -> Result<Option<Array<T>>> {
    if a.is_empty() || numbering.is_empty() {
        return Ok(None);
    }
    let n = numbering.len();
    if !a.len().is_multiple_of(n) {
        return Err(Error::Mesh(format!(
            "array '{}' has {} tuples, not a multiple of the {n} renumbered entities",
            a.id(),
            a.len()
        )));
    }
    let per = a.len() / n;
    let block = per * a.nb_component();
    let new_len = numbering.iter().filter(|&&i| i >= 0).count();
    let mut slots: Vec<Option<usize>> = vec![None; new_len];
    for (old, &new) in numbering.iter().enumerate() {
        if new >= 0 {
            let new = try_to_usize(new)?;
            let slot = slots.get_mut(new).ok_or(Error::OutOfRange {
                what: "new index",
                index: new,
                len: new_len,
            })?;
            if slot.replace(old).is_some() {
                return Err(Error::Mesh(format!(
                    "renumbering maps two entities to {new}"
                )));
            }
        }
    }
    let src = a.as_slice();
    let mut out = Vec::with_capacity(new_len * block);
    for old in slots {
        let old = old.ok_or_else(|| Error::Mesh("renumbering is not dense".into()))?;
        out.extend_from_slice(&src[old * block..(old + 1) * block]);
    }
    let id = a.id().to_owned();
    let nbc = a.nb_component();
    let mut c = Array::try_from_vec_with_len(out, new_len * per, nbc)?;
    c.set_id(id);
    Ok(Some(c))
}

fn compact_block<T: Clone>(
    m: &mut ElementTypeMapArray<T>,
    ty: ElementType,
    ghost: GhostType,
    numbering: &[Idx],
) -> Result<()> {
    match m.get_mut(ty, ghost) {
        Ok(a) => compact_rows(a, numbering),
        Err(_) => Ok(()),
    }
}

fn compact_shared_block<T: Clone>(
    m: &mut SharedElementTypeMapArray<T>,
    ty: ElementType,
    ghost: GhostType,
    numbering: &[Idx],
) -> Result<()> {
    match m.shared_mut(ty, ghost) {
        Ok(a) => compact_shared_rows(a, numbering),
        Err(_) => Ok(()),
    }
}

fn extend_shared_block<T: Clone + Default>(
    m: &mut SharedElementTypeMapArray<T>,
    ty: ElementType,
    ghost: GhostType,
    tuples: usize,
) -> Result<()> {
    if let Ok(mut a) = m.get_mut(ty, ghost)
        && a.len() < tuples
    {
        a.try_resize(tuples, T::default())?;
    }
    Ok(())
}

fn extend_plain_block<T: Clone>(
    m: &mut ElementTypeMapArray<T>,
    ty: ElementType,
    ghost: GhostType,
    tuples: usize,
    fill: T,
) -> Result<()> {
    if let Ok(a) = m.get_mut(ty, ghost)
        && a.len() < tuples
    {
        a.try_resize(tuples, fill)?;
    }
    Ok(())
}

impl MeshDataValues {
    /// Extend the block `(ty, ghost)`, if present, to `tuples` tuples with
    /// the type's empty value (`0`, `false`, `""`, `ElementNull`, an empty
    /// list): rows of elements appended without data (the ghost elements of
    /// a grid synchronizer).
    pub(crate) fn extend_block(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        tuples: usize,
    ) -> Result<()> {
        match self {
            Self::Int(m) => extend_shared_block(m, ty, ghost, tuples),
            Self::UInt(m) => extend_shared_block(m, ty, ghost, tuples),
            Self::Real(m) => extend_shared_block(m, ty, ghost, tuples),
            Self::Bool(m) => extend_shared_block(m, ty, ghost, tuples),
            Self::String(m) => extend_plain_block(m, ty, ghost, tuples, String::new()),
            Self::Element(m) => extend_plain_block(m, ty, ghost, tuples, Element::NULL),
            Self::ElementList(m) => extend_plain_block(m, ty, ghost, tuples, Vec::new()),
        }
    }

    /// Upstream type code of the dataset.
    #[must_use]
    pub fn type_code(&self) -> MeshDataTypeCode {
        match self {
            Self::Int(_) => MeshDataTypeCode::Int,
            Self::UInt(_) => MeshDataTypeCode::UInt,
            Self::Real(_) => MeshDataTypeCode::Real,
            Self::String(_) => MeshDataTypeCode::String,
            Self::Element(_) => MeshDataTypeCode::Element,
            Self::ElementList(_) => MeshDataTypeCode::ElementList,
            Self::Bool(_) => MeshDataTypeCode::Bool,
        }
    }

    /// `true` if the dataset has an array for `(ty, ghost)`.
    #[must_use]
    pub fn exists(&self, ty: ElementType, ghost: GhostType) -> bool {
        with_values!(self, m => m.exists(ty, ghost))
    }

    /// Number of tuples of `(ty, ghost)`, if present.
    #[must_use]
    pub fn len_of(&self, ty: ElementType, ghost: GhostType) -> Option<usize> {
        with_values!(self, m => m.as_map().get(ty, ghost).map(|a| a.len()))
    }

    /// Number of components of `(ty, ghost)`, if present.
    #[must_use]
    pub fn nb_component_of(&self, ty: ElementType, ghost: GhostType) -> Option<usize> {
        with_values!(self, m => m.as_map().get(ty, ghost).map(|a| a.nb_component()))
    }

    /// Present element types of one ghost block, in enum order.
    #[must_use]
    pub fn element_types(&self, ghost: GhostType) -> Vec<ElementType> {
        with_values!(self, m => m.element_types(akantu_core::TypeFilter::new().ghost(ghost)).collect())
    }

    /// Compact the rows of `(ty, ghost)` (see [`compact_rows`]).
    pub(crate) fn compact(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        numbering: &[Idx],
    ) -> Result<()> {
        match self {
            Self::Int(m) => compact_shared_block(m, ty, ghost, numbering),
            Self::UInt(m) => compact_shared_block(m, ty, ghost, numbering),
            Self::Real(m) => compact_shared_block(m, ty, ghost, numbering),
            Self::Bool(m) => compact_shared_block(m, ty, ghost, numbering),
            Self::String(m) => compact_block(m, ty, ghost, numbering),
            Self::Element(m) => compact_block(m, ty, ghost, numbering),
            Self::ElementList(m) => compact_block(m, ty, ghost, numbering),
        }
    }

    /// Map every element reference through `f` (no-op for non-element
    /// datasets).
    pub(crate) fn remap_elements(&mut self, f: &dyn Fn(Element) -> Element) {
        match self {
            Self::Element(m) => {
                for g in GhostType::ALL {
                    for (_, a) in m.as_map_mut().iter_mut(g) {
                        for e in a.as_mut_slice() {
                            *e = f(*e);
                        }
                    }
                }
            }
            Self::ElementList(m) => {
                for g in GhostType::ALL {
                    for (_, a) in m.as_map_mut().iter_mut(g) {
                        for list in a.as_mut_slice() {
                            for e in list.iter_mut() {
                                *e = f(*e);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Nodal data of one of the supported types.
#[derive(Clone, Debug)]
pub enum NodalDataValues {
    /// Int.
    Int(Array<Int>),
    /// UInt.
    UInt(Array<UInt>),
    /// Real.
    Real(Array<Real>),
}

impl NodalDataValues {
    /// Number of tuples.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Int(a) => a.len(),
            Self::UInt(a) => a.len(),
            Self::Real(a) => a.len(),
        }
    }

    /// `true` if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn compact(&mut self, numbering: &[Idx]) -> Result<()> {
        match self {
            Self::Int(a) => compact_rows(a, numbering),
            Self::UInt(a) => compact_rows(a, numbering),
            Self::Real(a) => compact_rows(a, numbering),
        }
    }
}

/// Named data store.
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    elemental: BTreeMap<String, MeshDataValues>,
    nodal: BTreeMap<String, NodalDataValues>,
    /// Elemental datasets holding per-element-node values (upstream
    /// `ElementTypeMapArray::isNodal()`, e.g. Gmsh `$ElementNodeData`).
    elemental_nodal: BTreeSet<String>,
}

/// Typed accessors `name(&self, dataset) -> &ElementTypeMapArray<T>` and
/// `name_mut_or_insert(&mut self, dataset) -> &mut …` (created empty on
/// first use; a dataset of another type is an error, as upstream).
macro_rules! typed_access {
    ($get:ident, $get_mut:ident, $insert:ident, $variant:ident, $t:ty, $label:literal) => {
        typed_access!($get, $get_mut, $insert, $variant, $t, $label, ElementTypeMapArray);
    };
    ($get:ident, $get_mut:ident, $insert:ident, $variant:ident, $t:ty, $label:literal, $etma:ident) => {
        #[doc = concat!("Elemental `", $label, "` dataset `name` (error if absent or of another type).")]
        pub fn $get(&self, name: &str) -> Result<&$etma<$t>> {
            match self.elemental(name)? {
                MeshDataValues::$variant(m) => Ok(m),
                other => Err(type_mismatch(name, other.type_code(), MeshDataTypeCode::$variant)),
            }
        }

        #[doc = concat!("Mutable elemental `", $label, "` dataset `name` (error if absent or of another type).")]
        pub fn $get_mut(&mut self, name: &str) -> Result<&mut $etma<$t>> {
            match self.elemental_mut(name)? {
                MeshDataValues::$variant(m) => Ok(m),
                other => Err(type_mismatch(name, other.type_code(), MeshDataTypeCode::$variant)),
            }
        }

        #[doc = concat!("Elemental `", $label, "` dataset `name`, created empty if absent (upstream `getElementalDataArrayAlloc` registers the type on first use).")]
        pub fn $insert(&mut self, name: &str) -> Result<&mut $etma<$t>> {
            let v = self
                .elemental
                .entry(name.to_owned())
                .or_insert_with(|| MeshDataValues::$variant($etma::new(name)));
            match v {
                MeshDataValues::$variant(m) => Ok(m),
                other => Err(type_mismatch(name, other.type_code(), MeshDataTypeCode::$variant)),
            }
        }
    };
}

fn type_mismatch(name: &str, has: MeshDataTypeCode, wanted: MeshDataTypeCode) -> Error {
    Error::Mesh(format!(
        "mesh data '{name}' has type {has:?}, accessed as {wanted:?}"
    ))
}

/// Write the id and every block of `m` **by value** (one line per block, in
/// enum order: `{prefix}{type}:{ghost} {Array Debug}`), for
/// [`crate::Mesh::state_fingerprint`] (D-071 (3)). The
/// blocks are read through [`SharedArray::borrow`]; their `Array` `Debug`
/// shows id, shape and values, never export metadata.
///
/// # Panics
/// As [`SharedArray::borrow`] (an exported block read outside a native
/// section of its runtime: protocol violation).
pub(crate) fn write_shared_blocks<T: std::fmt::Debug>(
    s: &mut String,
    prefix: &str,
    m: &SharedElementTypeMapArray<T>,
) {
    use std::fmt::Write as _;
    let _ = writeln!(s, "{prefix}id {:?}", m.id());
    for (ty, ghost, a) in m.as_map().iter_all() {
        let _ = writeln!(s, "{prefix}{ty}:{ghost} {:?}", *a.borrow());
    }
}

impl MeshData {
    /// Deterministic dump of every dataset **by value**, for
    /// [`crate::Mesh::state_fingerprint`] (D-071 (3)):
    /// elemental datasets in name order (the exportable numeric ones block by
    /// block through [`write_shared_blocks`], the plain ones through their
    /// value-printing `Debug`), nodal datasets in name order, then the set
    /// of per-element-node datasets. The destructuring pattern makes a new
    /// field a compile error here.
    ///
    /// # Panics
    /// As [`write_shared_blocks`].
    pub(crate) fn write_state(&self, s: &mut String) {
        use std::fmt::Write as _;
        let Self {
            elemental,
            nodal,
            elemental_nodal,
        } = self;
        for (name, values) in elemental {
            let _ = writeln!(s, "elemental {name:?} {:?}", values.type_code());
            match values {
                MeshDataValues::Int(m) => write_shared_blocks(s, "  ", m),
                MeshDataValues::UInt(m) => write_shared_blocks(s, "  ", m),
                MeshDataValues::Real(m) => write_shared_blocks(s, "  ", m),
                MeshDataValues::Bool(m) => write_shared_blocks(s, "  ", m),
                MeshDataValues::String(m) => {
                    let _ = writeln!(s, "  {m:?}");
                }
                MeshDataValues::Element(m) => {
                    let _ = writeln!(s, "  {m:?}");
                }
                MeshDataValues::ElementList(m) => {
                    let _ = writeln!(s, "  {m:?}");
                }
            }
        }
        for (name, values) in nodal {
            let _ = writeln!(s, "nodal {name:?} {values:?}");
        }
        let _ = writeln!(s, "elemental_nodal {elemental_nodal:?}");
    }

    /// Elemental data by name.
    pub fn elemental(&self, name: &str) -> Result<&MeshDataValues> {
        self.elemental
            .get(name)
            .ok_or_else(|| Error::not_found("mesh data", name))
    }

    /// Mutable elemental data by name.
    pub fn elemental_mut(&mut self, name: &str) -> Result<&mut MeshDataValues> {
        self.elemental
            .get_mut(name)
            .ok_or_else(|| Error::not_found("mesh data", name))
    }

    /// Insert (or replace) elemental data.
    pub fn insert_elemental(&mut self, name: impl Into<String>, values: MeshDataValues) {
        self.elemental.insert(name.into(), values);
    }

    /// Remove an elemental dataset (returns it).
    pub fn remove_elemental(&mut self, name: &str) -> Option<MeshDataValues> {
        self.elemental_nodal.remove(name);
        self.elemental.remove(name)
    }

    /// `true` if an elemental dataset `name` exists (upstream `hasData`).
    #[must_use]
    pub fn has_elemental(&self, name: &str) -> bool {
        self.elemental.contains_key(name)
    }

    /// Type code of the elemental dataset `name` (upstream `getTypeCode`).
    pub fn type_code(&self, name: &str) -> Result<MeshDataTypeCode> {
        self.elemental(name).map(MeshDataValues::type_code)
    }

    /// Names of the elemental datasets holding an array for `(ty, ghost)`,
    /// sorted (upstream `getTagNames(type, ghost_type)`).
    #[must_use]
    pub fn tag_names(&self, ty: ElementType, ghost: GhostType) -> Vec<&str> {
        self.elemental
            .iter()
            .filter(|(_, v)| v.exists(ty, ghost))
            .map(|(n, _)| n.as_str())
            .collect()
    }

    typed_access!(
        ints,
        ints_mut,
        ints_mut_or_insert,
        Int,
        Int,
        "Int",
        SharedElementTypeMapArray
    );
    typed_access!(
        uints,
        uints_mut,
        uints_mut_or_insert,
        UInt,
        UInt,
        "UInt",
        SharedElementTypeMapArray
    );
    typed_access!(
        reals,
        reals_mut,
        reals_mut_or_insert,
        Real,
        Real,
        "Real",
        SharedElementTypeMapArray
    );
    typed_access!(
        strings,
        strings_mut,
        strings_mut_or_insert,
        String,
        String,
        "String"
    );
    typed_access!(
        element_map,
        element_map_mut,
        element_map_mut_or_insert,
        Element,
        Element,
        "Element"
    );
    typed_access!(
        element_lists,
        element_lists_mut,
        element_lists_mut_or_insert,
        ElementList,
        Vec<Element>,
        "Vec<Element>"
    );
    typed_access!(
        bools,
        bools_mut,
        bools_mut_or_insert,
        Bool,
        bool,
        "bool",
        SharedElementTypeMapArray
    );

    /// Nodal data by name.
    pub fn nodal(&self, name: &str) -> Result<&NodalDataValues> {
        self.nodal
            .get(name)
            .ok_or_else(|| Error::not_found("nodal mesh data", name))
    }

    /// Insert (or replace) nodal data.
    pub fn insert_nodal(&mut self, name: impl Into<String>, values: NodalDataValues) {
        self.nodal.insert(name.into(), values);
    }

    /// Names of elemental data, sorted.
    pub fn elemental_names(&self) -> impl Iterator<Item = &str> {
        self.elemental.keys().map(String::as_str)
    }

    /// Mutable nodal data by name.
    pub fn nodal_mut(&mut self, name: &str) -> Result<&mut NodalDataValues> {
        self.nodal
            .get_mut(name)
            .ok_or_else(|| Error::not_found("nodal mesh data", name))
    }

    /// Names of nodal data, sorted.
    pub fn nodal_names(&self) -> impl Iterator<Item = &str> {
        self.nodal.keys().map(String::as_str)
    }

    /// `true` if no elemental or nodal dataset is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elemental.is_empty() && self.nodal.is_empty()
    }

    /// Mark (or unmark) the elemental dataset `name` as holding one tuple of
    /// values per element *node* (upstream `ElementTypeMapArray::isNodal`).
    pub fn set_elemental_nodal(&mut self, name: &str, nodal: bool) {
        if nodal {
            self.elemental_nodal.insert(name.to_owned());
        } else {
            self.elemental_nodal.remove(name);
        }
    }

    /// Whether the elemental dataset `name` holds per-element-node values.
    #[must_use]
    pub fn is_elemental_nodal(&self, name: &str) -> bool {
        self.elemental_nodal.contains(name)
    }

    /// Rows of the elements appended to `(ty, ghost)` without data: every
    /// elemental dataset holding that block is extended to `nb_elements`
    /// rows (`nb_nodes_per_element` tuples per row for per-element-node
    /// data) with its type's empty value. The grid synchronizer leaves the
    /// datasets of its new ghost elements short, as upstream (an oracle
    /// fact, `v101_grid_*`); a later element removal renumbers one row per
    /// element, so its caller pads them first (the non-local
    /// cleanup, inside its mesh mutation).
    ///
    /// # Errors
    /// Allocation errors.
    pub fn extend_elemental_rows(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        nb_elements: usize,
        nb_nodes_per_element: usize,
    ) -> Result<()> {
        let names: Vec<String> = self.elemental_names().map(str::to_owned).collect();
        for name in names {
            let per = if self.is_elemental_nodal(&name) {
                nb_nodes_per_element
            } else {
                1
            };
            self.elemental_mut(&name)?
                .extend_block(ty, ghost, nb_elements * per)?;
        }
        Ok(())
    }

    /// Element removal: compact the `(ty, ghost)` rows of every elemental
    /// dataset with `numbering` (old → new, `-1` removed).
    pub(crate) fn apply_element_numbering(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        numbering: &[Idx],
    ) -> Result<()> {
        for v in self.elemental.values_mut() {
            v.compact(ty, ghost, numbering)?;
        }
        Ok(())
    }

    /// Map every element reference of every elemental dataset whose name
    /// satisfies `select` through `f`.
    pub(crate) fn remap_element_references(
        &mut self,
        select: &dyn Fn(&str) -> bool,
        f: &dyn Fn(Element) -> Element,
    ) {
        for (name, v) in &mut self.elemental {
            if select(name) {
                v.remap_elements(f);
            }
        }
    }

    /// Node removal: compact every nodal dataset holding one or several
    /// tuples per node (other lengths are left alone, documented).
    pub(crate) fn apply_node_numbering(&mut self, numbering: &[Idx]) -> Result<()> {
        for v in self.nodal.values_mut() {
            if v.len().is_multiple_of(numbering.len().max(1)) {
                v.compact(numbering)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::{ElementType, GhostType};

    #[test]
    fn store_and_lookup() {
        let mut d = MeshData::default();
        let mut tags: ElementTypeMapArray<Int> = ElementTypeMapArray::new("tag_0");
        tags.alloc(2, 1, ElementType::Triangle3, GhostType::NotGhost, 7)
            .unwrap();
        d.insert_elemental("tag_0", MeshDataValues::Int(tags.into()));
        match d.elemental("tag_0").unwrap() {
            MeshDataValues::Int(t) => {
                assert_eq!(
                    t.get(ElementType::Triangle3, GhostType::NotGhost)
                        .unwrap()
                        .as_slice(),
                    &[7, 7]
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(d.elemental("tag_1").is_err());
        assert_eq!(d.elemental_names().collect::<Vec<_>>(), vec!["tag_0"]);
        assert!(!d.is_empty());
        assert!(!d.is_elemental_nodal("tag_0"));
        d.set_elemental_nodal("tag_0", true);
        assert!(d.is_elemental_nodal("tag_0"));
        d.set_elemental_nodal("tag_0", false);
        assert!(!d.is_elemental_nodal("tag_0"));
        d.insert_nodal("v", NodalDataValues::Real(Array::zeros(2, 1)));
        assert_eq!(d.nodal_names().collect::<Vec<_>>(), vec!["v"]);
        assert!(d.nodal_mut("v").is_ok() && d.nodal_mut("w").is_err());
        assert!(MeshData::default().is_empty());
    }

    /// Upstream `test_mesh_data` (typed: `string "5"/"10"`, `UInt 5/10`):
    /// values pushed through the allocating accessor are read back and the
    /// type code is reported; a typed access of the wrong type fails.
    #[test]
    fn test_mesh_data() {
        let mut d = MeshData::default();
        let ty = ElementType::Triangle6;
        let g = GhostType::NotGhost;
        let s = d.strings_mut_or_insert("string_data").unwrap();
        s.alloc(0, 1, ty, g, String::new()).unwrap();
        let a = s.get_mut(ty, g).unwrap();
        a.push_value("5".to_owned());
        a.push_value("10".to_owned());
        assert_eq!(
            d.strings("string_data")
                .unwrap()
                .get(ty, g)
                .unwrap()
                .as_slice(),
            &["5".to_owned(), "10".to_owned()]
        );
        assert_eq!(
            d.type_code("string_data").unwrap(),
            MeshDataTypeCode::String
        );
        let u = d.uints_mut_or_insert("UInt_data").unwrap();
        u.alloc(0, 1, ty, g, 0).unwrap();
        u.get_mut(ty, g).unwrap().push_value(5);
        u.get_mut(ty, g).unwrap().push_value(10);
        assert_eq!(
            d.uints("UInt_data").unwrap().get(ty, g).unwrap().as_slice(),
            &[5, 10]
        );
        assert_eq!(d.type_code("UInt_data").unwrap(), MeshDataTypeCode::UInt);
        assert!(matches!(
            d.ints_mut_or_insert("UInt_data"),
            Err(Error::Mesh(_))
        ));
        assert!(d.reals("string_data").is_err());
        assert_eq!(d.tag_names(ty, g), vec!["UInt_data", "string_data"]);
        assert!(d.tag_names(ElementType::Triangle3, g).is_empty());
        assert!(d.remove_elemental("UInt_data").is_some());
        assert!(!d.has_elemental("UInt_data"));
    }

    #[test]
    fn rows_follow_their_entity() {
        let mut a = Array::from_vec(vec![10, 11, 20, 21, 30, 31, 40, 41], 2).unwrap();
        // 4 entities, 1 tuple each of 2 components: remove 1, 3 → 0,2 kept.
        compact_rows(&mut a, &[0, -1, 1, -1]).unwrap();
        assert_eq!(a.as_slice(), &[10, 11, 30, 31]);
        // Two tuples per entity, hole filled by the last one.
        let mut q = Array::from_vec(vec![1, 2, 3, 4, 5, 6], 1).unwrap();
        compact_rows(&mut q, &[-1, 1, 0]).unwrap();
        assert_eq!(q.as_slice(), &[5, 6, 3, 4]);
        // Length mismatch and non-dense numberings are errors.
        let mut bad = Array::from_vec(vec![1, 2, 3], 1).unwrap();
        assert!(compact_rows(&mut bad, &[0, 1]).is_err());
        let mut b = Array::from_vec(vec![1, 2], 1).unwrap();
        assert!(compact_rows(&mut b, &[1, 1]).is_err());
        assert!(compact_rows(&mut b, &[2, 0]).is_err());
        let mut e: Array<i32> = Array::new(1);
        compact_rows(&mut e, &[0, -1]).unwrap();
    }
}
