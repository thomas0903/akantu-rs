// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/element_type_map_filter.hh
// (ElementTypeMapArrayFilter), src/common/aka_array_filter.hh (ArrayFilter).

//! Read-only views of per-type arrays restricted to element lists (MESH-020).
//!
//! Upstream builds an `ElementTypeMapArrayFilter` for every group-restricted
//! dumper field (`GroupManager::createElementalFilteredField`): a view of an
//! `ElementTypeMapArray<T>` through the element lists of a group
//! (`ElementTypeMapArray<Idx>`), optionally with a number of values per
//! element (`nb_data_per_elem`, e.g. `nb_quadrature_points × nb_component`)
//! that makes every element contribute several tuples. The port's dumpers
//! evaluate group fields by element identity instead (`docs/ARCHITECTURE.md`),
//! so this view is an independent utility with upstream's semantics:
//!
//! * `get(type, ghost)` (upstream `operator()`): for a type absent from the
//!   filter, an **empty** view; for a type of the filter, a view whose
//!   number of items per element is `nb_data_per_elem(type, ghost) /
//!   array.nb_component()` (integer division) when `nb_data_per_elem` has
//!   the type, else 1. A filter type missing from the array is an error
//!   (upstream throws from `ElementTypeMapArray::operator() const`).
//! * `element_types` iterates the **filter's** types; `nb_components` the
//!   **array's** (upstream forwards to `array.getNbComponents`).
//! * [`ArrayFilter::iter_reinterpret`] is upstream
//!   `begin_reinterpret(n, new_size)`: the array viewed as tuples of `n`
//!   values, `nb_component × filter.size() × nb_item_per_elem / (n ×
//!   new_size)` tuples per filtered element (integer division; unchanged
//!   when `n` or `new_size` is 0), tuple `items × element + k`. Upstream
//!   reads out of bounds when an index exceeds the reinterpreted array (or
//!   divides by zero for a zero-component reinterpretation of the empty
//!   view); the port checks every index first and returns an error
//!   (UD-668, category A).
//!
//! The element lists are a plain map or an exportable one (a group's
//! `ElementGroup::elements`, a `SharedElementTypeMapArray<Idx>`): see
//! [`FilterLists`]. A block of a shared map is read through one guard held
//! by its [`ArrayFilter`] (no copy; storage migration guide P17).

use akantu_core::{
    Array, ElementKind, ElementType, ElementTypeMap, ElementTypeMapArray, Error, GhostType, Idx,
    Result, SharedElementTypeMapArray, SliceRef, TypeFilter,
};

/// The element lists of an [`ElementTypeMapArrayFilter`] (upstream: an
/// `ElementTypeMapArray<Idx>`, typically a group's element lists).
#[derive(Clone, Copy, Debug)]
pub enum FilterLists<'a> {
    /// A plain map.
    Plain(&'a ElementTypeMapArray<Idx>),
    /// An exportable map (e.g. `ElementGroup::elements`).
    Shared(&'a SharedElementTypeMapArray<Idx>),
}

impl<'a> From<&'a ElementTypeMapArray<Idx>> for FilterLists<'a> {
    fn from(m: &'a ElementTypeMapArray<Idx>) -> Self {
        Self::Plain(m)
    }
}

impl<'a> From<&'a SharedElementTypeMapArray<Idx>> for FilterLists<'a> {
    fn from(m: &'a SharedElementTypeMapArray<Idx>) -> Self {
        Self::Shared(m)
    }
}

/// Read-only view of `array` through the element lists of `filter`
/// (upstream `ElementTypeMapArrayFilter<T>`).
#[derive(Debug)]
pub struct ElementTypeMapArrayFilter<'a, T> {
    array: &'a ElementTypeMapArray<T>,
    filter: FilterLists<'a>,
    nb_data_per_elem: ElementTypeMap<usize>,
}

impl<'a, T> ElementTypeMapArrayFilter<'a, T> {
    /// View without `nb_data_per_elem` (one item per filtered element);
    /// `filter` is a plain or an exportable map ([`FilterLists`]).
    #[must_use]
    pub fn new(array: &'a ElementTypeMapArray<T>, filter: impl Into<FilterLists<'a>>) -> Self {
        Self {
            array,
            filter: filter.into(),
            nb_data_per_elem: ElementTypeMap::new(),
        }
    }

    /// View with a number of values per element per `(type, ghost)`
    /// (upstream third constructor argument).
    #[must_use]
    pub fn with_nb_data_per_elem(
        array: &'a ElementTypeMapArray<T>,
        filter: impl Into<FilterLists<'a>>,
        nb_data_per_elem: ElementTypeMap<usize>,
    ) -> Self {
        Self {
            array,
            filter: filter.into(),
            nb_data_per_elem,
        }
    }

    /// The element list of `(ty, ghost)`, `None` if the filter has no such
    /// block (a shared block through one read guard).
    fn list(&self, ty: ElementType, ghost: GhostType) -> Result<Option<SliceRef<'a, Idx>>> {
        Ok(match self.filter {
            FilterLists::Plain(m) => m
                .as_map()
                .get(ty, ghost)
                .map(|a| SliceRef::from(a.as_slice())),
            FilterLists::Shared(m) => match m.as_map().get(ty, ghost) {
                Some(a) => Some(a.read()?.into_slice()),
                None => None,
            },
        })
    }

    /// Upstream `operator()(type, ghost_type)`.
    ///
    /// # Errors
    /// The filter lists `(ty, ghost)` but the array has no such block.
    pub fn get(&self, ty: ElementType, ghost: GhostType) -> Result<ArrayFilter<'a, T>> {
        let Some(list) = self.list(ty, ghost)? else {
            return Ok(ArrayFilter {
                array: None,
                filter: SliceRef::from(&[][..]),
                nb_item_per_elem: 1,
            });
        };
        let array = self.array.get(ty, ghost)?;
        let nb_item_per_elem = match self.nb_data_per_elem.get(ty, ghost) {
            Some(&n) => {
                let nc = array.nb_component();
                if nc == 0 {
                    return Err(Error::invalid(format!(
                        "ElementTypeMapArrayFilter: array block {ty}:{ghost} has no component \
                         (upstream divides by zero)"
                    )));
                }
                n / nc
            }
            None => 1,
        };
        Ok(ArrayFilter {
            array: Some(array),
            filter: list,
            nb_item_per_elem,
        })
    }

    /// Types of the **filter** passing `f`, in enum order (upstream
    /// `elementTypes(...)`).
    pub fn element_types(&self, f: TypeFilter) -> impl Iterator<Item = ElementType> + '_ {
        let types: Vec<ElementType> = match self.filter {
            FilterLists::Plain(m) => m.element_types(f).collect(),
            FilterLists::Shared(m) => m.element_types(f).collect(),
        };
        types.into_iter()
    }

    /// Number of components of every **array** block of the requested ghost
    /// type(s) (`ghost = None`: both, upstream `_casper`), dimension and kind
    /// (upstream `getNbComponents(dim, ghost, kind)`).
    #[must_use]
    pub fn nb_components(
        &self,
        spatial_dimension: Option<usize>,
        ghost: Option<GhostType>,
        kind: Option<ElementKind>,
    ) -> ElementTypeMap<usize> {
        nb_components(self.array, spatial_dimension, ghost, kind)
    }

    /// Upstream `getID()`: `"filtered:" + array id`.
    #[must_use]
    pub fn id(&self) -> String {
        format!("filtered:{}", self.array.id())
    }
}

/// Upstream `ElementTypeMapArray::getNbComponents(dim, ghost_type, kind)`:
/// the number of components of every block of `array` of the requested
/// ghost type(s) (`None`: both), dimension (`None`: all) and kind (`None`:
/// all), in enum order.
#[must_use]
pub fn nb_components<T>(
    array: &ElementTypeMapArray<T>,
    spatial_dimension: Option<usize>,
    ghost: Option<GhostType>,
    kind: Option<ElementKind>,
) -> ElementTypeMap<usize> {
    let mut out = ElementTypeMap::new();
    for g in GhostType::ALL {
        if ghost.is_some_and(|r| r != g) {
            continue;
        }
        let f = TypeFilter {
            spatial_dimension,
            ghost_type: g,
            kind: kind.filter(|k| *k != ElementKind::NotDefined),
        };
        for ty in array.element_types(f) {
            if let Some(a) = array.as_map().get(ty, g) {
                out.replace(ty, g, a.nb_component());
            }
        }
    }
    out
}

/// One block of an [`ElementTypeMapArrayFilter`] (upstream `ArrayFilter<T>`).
/// Holds the read guard of a shared element list while it lives.
#[derive(Debug)]
pub struct ArrayFilter<'a, T> {
    /// `None`: the empty view of a type absent from the filter.
    array: Option<&'a Array<T>>,
    filter: SliceRef<'a, Idx>,
    nb_item_per_elem: usize,
}

impl<'a, T> ArrayFilter<'a, T> {
    /// Upstream `size()`: `filter.size() × nb_item_per_elem`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.filter.len() * self.nb_item_per_elem
    }

    /// Upstream `empty()`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Upstream `getNbComponent()`: the array's (1 for the empty view, as
    /// upstream's default-constructed `Array`).
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.array.map_or(1, Array::nb_component)
    }

    /// Items per filtered element.
    #[must_use]
    pub fn nb_item_per_elem(&self) -> usize {
        self.nb_item_per_elem
    }

    /// The filtered element indices.
    #[must_use]
    pub fn filter(&self) -> &[Idx] {
        &self.filter
    }

    /// The `nb_item_per_elem` tuples (of the array's number of components)
    /// of every filtered element, in filter order: upstream
    /// `begin_reinterpret(nb_component, filter.size())`, i.e. tuple
    /// `nb_item_per_elem × e + k` of element `e`.
    ///
    /// # Errors
    /// See [`ArrayFilter::iter_reinterpret`].
    pub fn tuples(&self) -> Result<Vec<&'a [T]>> {
        self.iter_reinterpret(self.nb_component(), self.filter.len())
    }

    /// Upstream `begin_reinterpret(n, new_size)` … `end_reinterpret(n,
    /// new_size)`: the array as tuples of `n` values, `items` tuples per
    /// filtered element with `items = nb_component × filter.size() ×
    /// nb_item_per_elem / (n × new_size)` (integer division) when `n` and
    /// `new_size` are both non-zero, else `nb_item_per_elem`; the tuples of
    /// filtered element `e` are `items × e + k`, `k < items`.
    ///
    /// # Errors
    /// An index outside the reinterpreted array (upstream reads out of
    /// bounds), a negative filter entry, or, for a non-empty filter, `n = 0`
    /// or zero tuples per element (upstream divides by zero or never reaches
    /// `end`): UD-668.
    pub fn iter_reinterpret(&self, n: usize, new_size: usize) -> Result<Vec<&'a [T]>> {
        let items = if n != 0 && new_size != 0 {
            let total = self
                .nb_component()
                .checked_mul(self.filter.len())
                .and_then(|x| x.checked_mul(self.nb_item_per_elem))
                .ok_or_else(|| Error::invalid("ArrayFilter: size overflow"))?;
            let denom = n
                .checked_mul(new_size)
                .ok_or_else(|| Error::invalid("ArrayFilter: size overflow"))?;
            total / denom
        } else {
            self.nb_item_per_elem
        };
        let Some(array) = self.array else {
            return Ok(Vec::new());
        };
        if self.filter.is_empty() {
            return Ok(Vec::new());
        }
        if items == 0 || n == 0 {
            // upstream: the iterator never reaches `end` (items = 0) or
            // divides by zero (n = 0)
            return Err(Error::invalid(format!(
                "ArrayFilter::iter_reinterpret: {items} tuple(s) of {n} value(s) per element"
            )));
        }
        let count = self.filter.len() * items;
        let values = array.as_slice();
        let nb_tuples = values.len() / n;
        let mut out = Vec::with_capacity(count);
        for &e in self.filter.iter() {
            let e = usize::try_from(e)
                .map_err(|_| Error::invalid(format!("ArrayFilter: negative element index {e}")))?;
            for k in 0..items {
                let t = items
                    .checked_mul(e)
                    .and_then(|x| x.checked_add(k))
                    .filter(|t| *t < nb_tuples)
                    .ok_or(Error::OutOfRange {
                        what: "ArrayFilter tuple",
                        index: items.saturating_mul(e).saturating_add(k),
                        len: nb_tuples,
                    })?;
                out.push(&values[t * n..(t + 1) * n]);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::ElementType as Et;

    const NG: GhostType = GhostType::NotGhost;

    fn data() -> ElementTypeMapArray<f64> {
        let mut a = ElementTypeMapArray::new("p:data");
        // 4 triangles x 3 items x 2 components: value = 100 e + 10 q + c
        let t = a.alloc(12, 2, Et::Triangle3, NG, 0.0).unwrap();
        for e in 0..4u32 {
            for q in 0..3u32 {
                for c in 0..2u32 {
                    t.as_mut_slice()[((e * 3 + q) * 2 + c) as usize] =
                        f64::from(100 * e + 10 * q + c);
                }
            }
        }
        a
    }

    #[test]
    fn filtered_items_follow_upstream_order() {
        let a = data();
        let mut f = ElementTypeMapArray::<Idx>::new("p:filter");
        f.alloc(2, 1, Et::Triangle3, NG, 0)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&[2, 0]);
        // without nb_data_per_elem: one tuple per element (the e-th tuple)
        let plain = ElementTypeMapArrayFilter::new(&a, &f);
        let v = plain.get(Et::Triangle3, NG).unwrap();
        assert_eq!((v.len(), v.nb_component()), (2, 2));
        let t: Vec<Vec<f64>> = v.tuples().unwrap().iter().map(|x| x.to_vec()).collect();
        assert_eq!(t, vec![vec![20.0, 21.0], vec![0.0, 1.0]]);
        drop(v); // not shadowed: the view holds the filter block (P12)
        // with nb_data_per_elem = 6: three tuples per element
        let mut nbd = ElementTypeMap::new();
        nbd.insert(Et::Triangle3, NG, 6).unwrap();
        let full = ElementTypeMapArrayFilter::with_nb_data_per_elem(&a, &f, nbd);
        let v = full.get(Et::Triangle3, NG).unwrap();
        assert_eq!(v.len(), 6);
        let t: Vec<f64> = v.tuples().unwrap().concat();
        assert_eq!(
            t,
            vec![
                200.0, 201.0, 210.0, 211.0, 220.0, 221.0, 0.0, 1.0, 10.0, 11.0, 20.0, 21.0
            ]
        );
        assert_eq!(full.id(), "filtered:p:data");
        // absent type: empty view; filter type absent from the array: error
        assert!(full.get(Et::Quadrangle4, NG).unwrap().is_empty());
        drop(v); // the view holds the filter block (P12)
        f.alloc(1, 1, Et::Quadrangle4, NG, 0).unwrap();
        let bad = ElementTypeMapArrayFilter::new(&a, &f);
        assert!(bad.get(Et::Quadrangle4, NG).is_err());
        assert_eq!(
            bad.element_types(TypeFilter::new()).collect::<Vec<_>>(),
            vec![Et::Triangle3, Et::Quadrangle4]
        );
    }

    #[test]
    fn out_of_range_reinterpretation_is_an_error() {
        let a = data();
        let mut f = ElementTypeMapArray::<Idx>::new("p:filter");
        f.alloc(1, 1, Et::Triangle3, NG, 0)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&[11]);
        let v = ElementTypeMapArrayFilter::new(&a, &f)
            .get(Et::Triangle3, NG)
            .unwrap();
        // element 11 < 12 tuples: fine; as 3-value tuples (8 tuples): error
        assert_eq!(v.tuples().unwrap().len(), 1);
        assert!(v.iter_reinterpret(3, 1).is_err());
        drop(v);
        f.as_map_mut()
            .get_mut(Et::Triangle3, NG)
            .unwrap()
            .as_mut_slice()[0] = -1;
        let v = ElementTypeMapArrayFilter::new(&a, &f)
            .get(Et::Triangle3, NG)
            .unwrap();
        assert!(v.tuples().is_err());
    }

    /// UD-668 bounds check (reproducer
    /// `v88_filter_bounds.rs`): a filter entry past the array with a
    /// non-zero number of tuples per element is `OutOfRange`, never a panic
    /// (upstream reads out of bounds). Reaches the `t < nb_tuples` check
    /// itself (not the `items == 0` or negative-entry paths).
    #[test]
    fn filter_entry_past_the_array_is_out_of_range() {
        let mut a = ElementTypeMapArray::<f64>::new("p:data");
        a.alloc(4, 2, Et::Triangle3, NG, 1.0).unwrap(); // 4 tuples of 2 values
        let mut f = ElementTypeMapArray::<Idx>::new("p:filter");
        f.alloc(1, 1, Et::Triangle3, NG, 3).unwrap(); // the last element
        let view = ElementTypeMapArrayFilter::new(&a, &f);
        let v = view.get(Et::Triangle3, NG).unwrap();
        assert_eq!(v.nb_item_per_elem(), 1);
        assert_eq!(v.tuples().unwrap(), vec![&[1.0, 1.0][..]]);
        assert_eq!(v.iter_reinterpret(1, 1).unwrap().len(), 2); // tuples 6, 7 of 8
        drop(v);
        f.as_map_mut()
            .get_mut(Et::Triangle3, NG)
            .unwrap()
            .as_mut_slice()[0] = 4; // past the end
        let view = ElementTypeMapArrayFilter::new(&a, &f);
        let v = view.get(Et::Triangle3, NG).unwrap();
        // one tuple of 2 values per element: tuple 4 of 4
        assert!(matches!(
            v.tuples(),
            Err(Error::OutOfRange {
                index: 4,
                len: 4,
                ..
            })
        ));
        // 2 tuples of 1 value per element: tuple 8 of 8
        assert!(matches!(
            v.iter_reinterpret(1, 1),
            Err(Error::OutOfRange {
                index: 8,
                len: 8,
                ..
            })
        ));
    }

    #[test]
    fn nb_components_of_the_array() {
        let mut a = data();
        a.alloc(1, 5, Et::Segment2, GhostType::Ghost, 0.0).unwrap();
        let m = nb_components(&a, None, None, None);
        assert_eq!(m.get(Et::Triangle3, NG), Some(&2));
        assert_eq!(m.get(Et::Segment2, GhostType::Ghost), Some(&5));
        let m = nb_components(&a, Some(2), Some(NG), None);
        assert_eq!(m.get(Et::Segment2, GhostType::Ghost), None);
        assert_eq!(m.get(Et::Triangle3, NG), Some(&2));
    }
}
