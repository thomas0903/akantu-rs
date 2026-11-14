// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/element_type_map.hh,
// element_type_map_tmpl.hh (ElementTypeMapArray<T>), with exportable blocks
// (akantu-rs design: decision records D-024, D-047; docs/ARCHITECTURE.md, "Arrays" and "Shared storage and its aliasing rules").

//! [`SharedElementTypeMapArray`]: an `ElementTypeMapArray` whose blocks are
//! [`SharedArray`]s, for the per-type fields that foreign code (NumPy, C)
//! may view zero-copy — mesh connectivities and data, group element lists,
//! constitutive-law internal fields, element filters, law-by-element maps.
//!
//! The API follows [`ElementTypeMapArray`] (same ids, errors, allocation
//! rules and enum-order iteration), except that element access goes through
//! the guards of the foreign-view protocol: [`SharedElementTypeMapArray::get`]
//! returns an [`ArrayRef`], [`SharedElementTypeMapArray::get_mut`] an
//! [`ArrayMut`]. Guards on unexported blocks are free.

use std::fmt;

use crate::array::Array;
use crate::array_shared::{ArrayMut, ArrayRef, SharedArray, SliceRef};
use crate::element::Element;
use crate::element_type::{ElementType, GhostType};
use crate::element_type_map::{ElementTypeMap, ElementTypeMapArray, TypeFilter};
use crate::error::{Error, Result};
use crate::types::Id;

/// Per-type exportable arrays (upstream `ElementTypeMapArray<T>`).
///
/// `Clone` copies every block through [`SharedArray`]'s `Clone` (fresh,
/// unexported allocations).
///
/// # Panics
/// `clone` and `==` read the blocks with [`SharedArray::borrow`]: they panic
/// if a block is exported and no native section of its runtime is active (a
/// protocol violation of the exporting code, `RefCell` semantics; ARCHITECTURE.md, "Shared storage and its aliasing rules").
#[derive(Clone, Default)]
pub struct SharedElementTypeMapArray<T> {
    map: ElementTypeMap<SharedArray<T>>,
    id: Id,
}

impl<T> SharedElementTypeMapArray<T> {
    /// Empty container with an id (upstream `parent_id:id`).
    #[must_use]
    pub fn new(id: impl Into<Id>) -> Self {
        Self {
            map: ElementTypeMap::new(),
            id: id.into(),
        }
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Underlying map of exportable blocks.
    #[must_use]
    pub fn as_map(&self) -> &ElementTypeMap<SharedArray<T>> {
        &self.map
    }

    /// Underlying mutable map of exportable blocks.
    pub fn as_map_mut(&mut self) -> &mut ElementTypeMap<SharedArray<T>> {
        &mut self.map
    }

    /// Upstream `exists`.
    #[must_use]
    pub fn exists(&self, ty: ElementType, ghost: GhostType) -> bool {
        self.map.exists(ty, ghost)
    }

    fn missing(&self, ty: ElementType, ghost: GhostType) -> Error {
        Error::not_found("element type array", format!("{}:{ty}:{ghost}", self.id))
    }

    /// The exportable block of `(ty, ghost)`; error if absent.
    #[inline]
    pub fn shared(&self, ty: ElementType, ghost: GhostType) -> Result<&SharedArray<T>> {
        self.map
            .get(ty, ghost)
            .ok_or_else(|| self.missing(ty, ghost))
    }

    /// The mutable exportable block of `(ty, ghost)` (export, guards).
    #[inline]
    pub fn shared_mut(&mut self, ty: ElementType, ghost: GhostType) -> Result<&mut SharedArray<T>> {
        // the error (a formatted id) is built only when the block is absent
        let id = &self.id;
        self.map
            .get_mut(ty, ghost)
            .ok_or_else(|| Error::not_found("element type array", format!("{id}:{ty}:{ghost}")))
    }

    /// Shared guard of `(ty, ghost)`; error if absent (upstream throws) or
    /// if the block is exported and no native section is active.
    #[inline]
    pub fn get(&self, ty: ElementType, ghost: GhostType) -> Result<ArrayRef<'_, T>> {
        self.shared(ty, ghost)?.read()
    }

    /// Exclusive guard of `(ty, ghost)` (same admission rule as
    /// [`SharedElementTypeMapArray::get`]).
    #[inline]
    pub fn get_mut(&mut self, ty: ElementType, ghost: GhostType) -> Result<ArrayMut<'_, T>> {
        self.shared_mut(ty, ghost)?.write()
    }

    /// Present types passing `filter`, in enum order.
    pub fn element_types(&self, filter: TypeFilter) -> impl Iterator<Item = ElementType> + '_ {
        self.map.element_types(filter)
    }

    /// Remove a block (an exported block keeps its views stale-but-safe
    /// when the returned array is dropped).
    pub fn remove(&mut self, ty: ElementType, ghost: GhostType) -> Option<SharedArray<T>> {
        self.map.remove(ty, ghost)
    }

    /// Remove all blocks (upstream `free`).
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Total number of tuples over all types and ghost types (metadata).
    #[must_use]
    pub fn total_len(&self) -> usize {
        self.map.iter_all().map(|(_, _, a)| a.len()).sum()
    }

    /// `true` while any block is exported.
    #[must_use]
    pub fn is_exported(&self) -> bool {
        self.map.iter_all().any(|(_, _, a)| a.is_exported())
    }

    /// The tuple of `element` as a guard (no copy):
    /// same errors as [`SharedElementTypeMapArray::element_to_vec`]. This is
    /// the accessor for per-element code (it never allocates).
    #[inline]
    pub fn element_ref(&self, element: Element) -> Result<SliceRef<'_, T>> {
        let i = element
            .try_idx()
            .ok_or_else(|| Error::invalid("ElementTypeMapArray::element: ElementNull"))?;
        self.get(element.ty(), element.ghost_type())?.into_tuple(i)
    }

    /// The values of block `(ty, ghost)` as a guard (no copy), or the empty
    /// slice if the block is absent.
    pub fn slice_or_empty(&self, ty: ElementType, ghost: GhostType) -> Result<SliceRef<'_, T>> {
        match self.map.get(ty, ghost) {
            Some(a) => Ok(a.read()?.into_slice()),
            None => Ok(SliceRef::empty()),
        }
    }
}

impl<T: Copy> SharedElementTypeMapArray<T> {
    /// **Copy** of the tuple of `element` (upstream `get(element)`) into a
    /// new `Vec`; error for [`Element::NULL`], an absent block or an
    /// out-of-range index. Per-element code uses the non-allocating
    /// [`SharedElementTypeMapArray::element_ref`]. Named `element_to_vec`,
    /// not `element` like the borrowing accessor of
    /// [`ElementTypeMapArray`], so that turning a plain map into a shared one
    /// can never silently turn a borrow into an allocation
    /// (D-071 (4)).
    pub fn element_to_vec(&self, element: Element) -> Result<Vec<T>> {
        let i = element
            .try_idx()
            .ok_or_else(|| Error::invalid("ElementTypeMapArray::element: ElementNull"))?;
        Ok(self
            .get(element.ty(), element.ghost_type())?
            .get_tuple(i)?
            .to_vec())
    }

    /// Exclusive guard of the block of `element` and the element's tuple
    /// index (validated against the block length).
    pub fn element_guard_mut(&mut self, element: Element) -> Result<(ArrayMut<'_, T>, usize)> {
        let i = element
            .try_idx()
            .ok_or_else(|| Error::invalid("ElementTypeMapArray::element_mut: ElementNull"))?;
        let a = self.get_mut(element.ty(), element.ghost_type())?;
        if i >= a.len() {
            return Err(Error::OutOfRange {
                what: "element",
                index: i,
                len: a.len(),
            });
        }
        Ok((a, i))
    }

    /// Overwrite the tuple of `element` with `values` (same length).
    pub fn set_element(&mut self, element: Element, values: &[T]) -> Result<()> {
        let i = element
            .try_idx()
            .ok_or_else(|| Error::invalid("ElementTypeMapArray::set_element: ElementNull"))?;
        let mut a = self.get_mut(element.ty(), element.ghost_type())?;
        let t = a.get_tuple_mut(i)?;
        if t.len() != values.len() {
            return Err(Error::DimensionMismatch {
                context: "ElementTypeMapArray::set_element",
                expected: t.len(),
                got: values.len(),
            });
        }
        t.copy_from_slice(values);
        Ok(())
    }
}

impl<T: Clone> SharedElementTypeMapArray<T> {
    /// Allocate `(ty, ghost)` with `len` tuples of `nb_component` values, or
    /// resize the existing block (upstream `alloc`; rules of
    /// [`ElementTypeMapArray::alloc`]: ids `"{id}:{type}"` plus `":ghost"`,
    /// a different `nb_component` is rejected, sizes checked, nothing changed
    /// on error). Resizing an exported block follows ARCHITECTURE.md, "Shared storage and its aliasing rules" (growth beyond
    /// capacity and any shrink retire it).
    pub fn alloc(
        &mut self,
        len: usize,
        nb_component: usize,
        ty: ElementType,
        ghost: GhostType,
        default: T,
    ) -> Result<ArrayMut<'_, T>> {
        if let Some(a) = self.map.get(ty, ghost)
            && a.nb_component() != nb_component
        {
            return Err(Error::DimensionMismatch {
                context: "ElementTypeMapArray::alloc",
                expected: a.nb_component(),
                got: nb_component,
            });
        }
        if !self.map.exists(ty, ghost) {
            let mut a = Array::try_filled(len, nb_component, default)?;
            let ghost_suffix = if ghost == GhostType::Ghost {
                ":ghost"
            } else {
                ""
            };
            a.set_id(format!("{}:{ty}{ghost_suffix}", self.id));
            return self.map.insert(ty, ghost, SharedArray::new(a))?.write();
        }
        let mut a = self.get_mut(ty, ghost)?;
        a.try_resize(len, default)?;
        Ok(a)
    }

    /// Allocate/resize several blocks at once (see
    /// [`ElementTypeMapArray::initialize`]).
    pub fn initialize<I>(&mut self, blocks: I, default: T) -> Result<()>
    where
        I: IntoIterator<Item = (ElementType, GhostType, usize, usize)>,
    {
        for (ty, g, len, nbc) in blocks {
            self.alloc(len, nbc, ty, g, default.clone())?;
        }
        Ok(())
    }

    /// Set every value to `value` (upstream `set`).
    pub fn fill(&mut self, value: T) -> Result<()> {
        for g in GhostType::ALL {
            for (_, a) in self.map.iter_mut(g) {
                a.write()?.fill(value.clone());
            }
        }
        Ok(())
    }

    /// A plain copy (for code taking an [`ElementTypeMapArray`]).
    pub fn to_plain(&self) -> Result<ElementTypeMapArray<T>> {
        let mut out = ElementTypeMapArray::new(self.id.clone());
        for (t, g, a) in self.map.iter_all() {
            out.as_map_mut().insert(t, g, a.read()?.clone())?;
        }
        Ok(out)
    }
}

impl<T> From<ElementTypeMapArray<T>> for SharedElementTypeMapArray<T> {
    fn from(etma: ElementTypeMapArray<T>) -> Self {
        let id = etma.id().to_owned();
        let mut map = ElementTypeMap::new();
        let mut src = etma;
        for g in GhostType::ALL {
            for t in ElementType::ALL {
                if let Some(a) = src.remove(t, g) {
                    map.replace(t, g, SharedArray::new(a));
                }
            }
        }
        Self { map, id }
    }
}

impl<T: PartialEq> PartialEq for SharedElementTypeMapArray<T> {
    /// Same id and the same blocks (values compared through
    /// [`SharedArray::borrow`]).
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.map == other.map
    }
}

impl<T> fmt::Debug for SharedElementTypeMapArray<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedElementTypeMapArray")
            .field("id", &self.id)
            .field("map", &self.map)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::array_shared::ForeignRuntime;
    use crate::element_type::ElementType as Et;

    #[test]
    fn alloc_get_and_errors_follow_element_type_map_array() {
        let mut m = SharedElementTypeMapArray::<i32>::new("mesh:connectivities");
        m.alloc(2, 3, Et::Triangle3, GhostType::NotGhost, -1)
            .unwrap()
            .as_mut_slice()[0] = 7;
        assert_eq!(
            m.get(Et::Triangle3, GhostType::NotGhost).unwrap().id(),
            "mesh:connectivities:_triangle_3"
        );
        m.alloc(1, 3, Et::Triangle3, GhostType::Ghost, 0).unwrap();
        assert_eq!(
            m.get(Et::Triangle3, GhostType::Ghost).unwrap().id(),
            "mesh:connectivities:_triangle_3:ghost"
        );
        assert!(
            m.alloc(2, 4, Et::Triangle3, GhostType::NotGhost, 0)
                .is_err()
        );
        assert!(m.get(Et::Quadrangle4, GhostType::NotGhost).is_err());
        assert_eq!(m.total_len(), 3);
        assert_eq!(
            m.element_to_vec(Element::new(Et::Triangle3, 0, GhostType::NotGhost))
                .unwrap(),
            vec![7, -1, -1]
        );
        let plain = m.to_plain().unwrap();
        assert_eq!(
            plain
                .get(Et::Triangle3, GhostType::NotGhost)
                .unwrap()
                .as_slice(),
            &[7, -1, -1, -1, -1, -1]
        );
        let back = SharedElementTypeMapArray::from(plain);
        assert_eq!(back, m);
    }

    #[test]
    fn exported_blocks_need_a_native_section() {
        let rt = ForeignRuntime::new();
        let mut m = SharedElementTypeMapArray::<f64>::new("f");
        m.alloc(2, 1, Et::Segment2, GhostType::NotGhost, 0.0)
            .unwrap();
        let lease = m
            .shared_mut(Et::Segment2, GhostType::NotGhost)
            .unwrap()
            .export(&rt)
            .unwrap();
        assert!(m.is_exported());
        assert!(m.get(Et::Segment2, GhostType::NotGhost).is_err());
        let mut interp = rt.enter().unwrap();
        {
            let _s = interp.native().unwrap();
            m.fill(2.0).unwrap();
            assert_eq!(
                m.get(Et::Segment2, GhostType::NotGhost).unwrap().as_slice(),
                &[2.0, 2.0]
            );
        }
        lease.check_foreign_access(&interp).unwrap();
        drop(lease);
        assert!(!m.is_exported());
    }
}
