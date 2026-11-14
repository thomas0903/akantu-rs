// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_mesh.cc:43-150
// (register_element_type_map_array<T>: ElementTypeMapArrayReal/UInt/Int/
// bool with `__call__`, `elementTypes`, `initialize`, `getID`),
// src/mesh/element_type_map_tmpl.hh (initialize with a mesh).

//! `ElementTypeMapArray{Real,UInt,Int,bool}` (PY-055).
//!
//! An instance either **owns** its arrays (created from Python:
//! `ElementTypeMapArrayReal()`, then `initialize(mesh, …)`; the arrays are
//! [`SharedArray`]s exported zero-copy) or **refers** to arrays owned by a
//! mesh or a model (`mesh.getConnectivities()`, `group.getElements()`,
//! `mesh.getElementalDataUInt(name)`, `model.getMaterialByElement()`,
//! material internals), returned as zero-copy views of the owner's storage
//! (D-047).

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

use akantu::core::array_shared::SharedArray;
use akantu::core::{Array, ElementType, Error, GhostType, TypeFilter};
use pyo3::prelude::*;

use super::enums::{EtArg, GhostArg, KindArg, element_type_obj};
use super::exports::into_numpy;
use super::mesh::{MeshKey, MeshRef, MeshValue, PY_ALL_DIMENSIONS, PyMesh, dim_filter, select};
use super::runtime::{native, runtime};
use crate::foreign_view::numpy_export::{Exportable, Layout};
use crate::foreign_view::{ForeignView, SharedOwner};

/// Arrays reachable through an `ElementTypeMapArray*` object.
pub(crate) trait EtmaProvider<T: Exportable + Default + Debug>: Send + Sync + Debug {
    /// `getID()`.
    fn id(&self, py: Python<'_>) -> PyResult<String>;
    /// Types present for a filter.
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>>;
    /// `self(type, ghost)`: the array as NumPy.
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>>;
    /// The owned storage, for `initialize`.
    fn owned(&self) -> Option<&Arc<SharedOwner<OwnEtma<T>>>> {
        None
    }
}

/// Arrays owned by a Python-created `ElementTypeMapArray*`.
#[derive(Debug)]
pub(crate) struct OwnEtma<T> {
    id: String,
    arrays: BTreeMap<(GhostType, ElementType), SharedArray<T>>,
}

impl<T: Exportable + Default> OwnEtma<T> {
    /// Write `values` into the block `(ty, ghost)` (created if absent;
    /// native section required when the block is exported): in place for
    /// the same shape (attached views see the new values), otherwise the
    /// block is replaced (ARCHITECTURE.md, "Shared storage and its aliasing rules": its views go stale).
    pub(crate) fn put(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        values: Array<T>,
    ) -> akantu::core::Result<()> {
        let id = format!(
            "{}:{ty}{}",
            self.id,
            if ghost == GhostType::Ghost {
                ":ghost"
            } else {
                ""
            }
        );
        match self.arrays.get_mut(&(ghost, ty)) {
            Some(a) => {
                let mut w = a.write()?;
                if w.len() == values.len() && w.nb_component() == values.nb_component() {
                    w.copy_from(&values)?;
                } else {
                    let mut values = values;
                    values.set_id(id);
                    w.replace(values);
                }
            }
            None => {
                let mut values = values;
                values.set_id(id);
                self.arrays.insert((ghost, ty), SharedArray::new(values));
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct OwnProvider<T>(Arc<SharedOwner<OwnEtma<T>>>);

impl<T: Exportable + Default + Debug> EtmaProvider<T> for OwnProvider<T> {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        let o = Arc::clone(&self.0);
        native(py, move || Ok(o.try_borrow_mut()?.id.clone()))
    }

    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        let o = Arc::clone(&self.0);
        native(py, move || {
            let o = o.try_borrow_mut()?;
            Ok(o.arrays
                .keys()
                .filter(|(g, t)| *g == filter.ghost_type && filter.accepts(*t))
                .map(|(_, t)| *t)
                .collect())
        })
    }

    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = Arc::clone(&self.0);
        let e = native(py, move || {
            let mut o = o.try_borrow_mut()?;
            let id = o.id.clone();
            let a = o.arrays.get_mut(&(ghost, ty)).ok_or_else(|| {
                Error::not_found(
                    "element type map array block",
                    format!(
                        "{id}:{ty}{}",
                        if ghost == GhostType::Ghost {
                            ":ghost"
                        } else {
                            ""
                        }
                    ),
                )
            })?;
            ForeignView::new(a, runtime(), true)
        })?;
        into_numpy(py, e, Layout::Rows)
    }

    fn owned(&self) -> Option<&Arc<SharedOwner<OwnEtma<T>>>> {
        Some(&self.0)
    }
}

/// Mesh-owned element type maps.
#[derive(Clone, Debug)]
pub(crate) enum EtmaSource {
    /// `mesh.getConnectivities()` (`Int`, writable).
    Connectivities(MeshRef),
    /// `mesh.getElementalData*(name)` (writable).
    Data(MeshRef, String),
    /// `group.getElements()` (`Int`, read-only).
    GroupElements(MeshRef, String),
}

impl EtmaSource {
    fn mesh(&self) -> &MeshRef {
        match self {
            Self::Connectivities(m) | Self::Data(m, _) | Self::GroupElements(m, _) => m,
        }
    }
}

#[derive(Debug)]
struct MeshProvider(EtmaSource);

impl<T: MeshValue> EtmaProvider<T> for MeshProvider {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        let src = self.0.clone();
        let mesh = src.mesh().clone();
        mesh.read(py, move |m| {
            Ok(match &src {
                EtmaSource::Connectivities(_) => m.connectivities().id().to_owned(),
                EtmaSource::Data(_, n) => n.clone(),
                EtmaSource::GroupElements(_, n) => m
                    .groups()
                    .element_group(n)?
                    .group()
                    .elements
                    .id()
                    .to_owned(),
            })
        })
    }

    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        let src = self.0.clone();
        let mesh = src.mesh().clone();
        let part = mesh.part;
        mesh.read(py, move |m| {
            let _ = select(m, part)?;
            Ok(match &src {
                EtmaSource::Connectivities(_) => m.connectivities().element_types(filter).collect(),
                EtmaSource::Data(_, n) => m
                    .data()
                    .elemental(n)?
                    .element_types(filter.ghost_type)
                    .into_iter()
                    .filter(|t| filter.accepts(*t))
                    .collect(),
                EtmaSource::GroupElements(_, n) => m
                    .groups()
                    .element_group(n)?
                    .group()
                    .elements
                    .element_types(filter)
                    .collect(),
            })
        })
    }

    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        let key = match &self.0 {
            EtmaSource::Connectivities(_) => MeshKey::Conn(ty, ghost),
            EtmaSource::Data(_, n) => MeshKey::Data(n.clone(), ty, ghost, T::DATA),
            EtmaSource::GroupElements(_, n) => MeshKey::GroupElements(n.clone(), ty, ghost),
        };
        self.0.mesh().view::<T>(py, key, Layout::Rows)
    }
}

/// `ElementTypeMapArray(…).initialize(mesh, …)` on owned storage.
#[allow(clippy::too_many_arguments)] // upstream named-argument list
fn initialize_owned<T: Exportable + Default + Debug>(
    py: Python<'_>,
    own: &Arc<SharedOwner<OwnEtma<T>>>,
    mesh: &MeshRef,
    ghost: Option<GhostType>,
    nb_component: usize,
    spatial_dimension: i64,
    kind: KindArg,
    with_nb_element: bool,
    with_nb_nodes_per_element: bool,
    default_value: T,
    do_not_default: bool,
) -> PyResult<()> {
    let own = Arc::clone(own);
    let mesh_h = mesh.clone();
    let blocks = mesh.read(py, move |m| {
        let dim = if spatial_dimension == -2 {
            Some(m.spatial_dimension())
        } else if spatial_dimension == PY_ALL_DIMENSIONS {
            None
        } else {
            Some(usize::try_from(spatial_dimension).map_err(|_| {
                Error::invalid(format!("invalid spatial dimension {spatial_dimension}"))
            })?)
        };
        let ghosts: Vec<GhostType> = match ghost {
            Some(g) => vec![g],
            None => GhostType::ALL.to_vec(),
        };
        let mut blocks = Vec::new();
        for g in ghosts {
            for t in m.element_types(TypeFilter {
                spatial_dimension: dim,
                ghost_type: g,
                kind: kind.0,
            }) {
                let nbc = nb_component
                    * if with_nb_nodes_per_element {
                        t.nb_nodes_per_element()
                    } else {
                        1
                    };
                let len = if with_nb_element {
                    m.nb_element(t, g)
                } else {
                    0
                };
                blocks.push((t, g, len, nbc));
            }
        }
        Ok(blocks)
    })?;
    let _ = mesh_h;
    write_blocks(py, own, blocks, default_value, do_not_default)
}

/// Create or resize the blocks `(type, ghost, rows, nb_component)` of an
/// owned map (upstream `ElementTypeMapArray::initialize(functor, …)`).
fn write_blocks<T: Exportable + Default + Debug>(
    py: Python<'_>,
    own: Arc<SharedOwner<OwnEtma<T>>>,
    blocks: Vec<(ElementType, GhostType, usize, usize)>,
    default_value: T,
    do_not_default: bool,
) -> PyResult<()> {
    native(py, move || {
        let mut o = own.try_borrow_mut()?;
        let id = o.id.clone();
        for (t, g, len, nbc) in blocks {
            let fill = if do_not_default {
                T::default()
            } else {
                default_value
            };
            match o.arrays.get_mut(&(g, t)) {
                Some(a) => {
                    let mut w = a.write()?;
                    if w.nb_component() != nbc {
                        return Err(Error::DimensionMismatch {
                            context: "ElementTypeMapArray.initialize",
                            expected: w.nb_component(),
                            got: nbc,
                        });
                    }
                    w.try_resize(len, fill)?;
                }
                None => {
                    let mut a = Array::try_filled(len, nbc, fill)?;
                    a.set_id(format!(
                        "{id}:{t}{}",
                        if g == GhostType::Ghost { ":ghost" } else { "" }
                    ));
                    o.arrays.insert((g, t), SharedArray::new(a));
                }
            }
        }
        Ok(())
    })
}

/// The first argument of `initialize` and the positional arguments after
/// it, checked against upstream's three overloads (see `initialize`): more
/// than one positional argument, or `fe_engine=`, require an `FEEngine`.
fn initialize_target<'py>(
    args: &Bound<'py, pyo3::types::PyTuple>,
    mesh: Option<Bound<'py, PyAny>>,
    fe_engine: Option<Bound<'py, PyAny>>,
) -> PyResult<(Bound<'py, PyAny>, Vec<Bound<'py, PyAny>>)> {
    use pyo3::exceptions::PyTypeError;
    let n = args.len();
    if n > 9 {
        return Err(PyTypeError::new_err(format!(
            "initialize() takes at most 9 positional arguments ({n} given)"
        )));
    }
    let (target, needs_fe) = match (n, mesh, fe_engine) {
        (0, Some(m), None) => (m, false),
        (0, None, Some(f)) => (f, true),
        (0, None, None) => {
            return Err(PyTypeError::new_err(
                "initialize(): missing the mesh or fe_engine argument",
            ));
        }
        (_, None, None) => (args.get_item(0)?, n > 1),
        _ => {
            return Err(PyTypeError::new_err(
                "initialize(): incompatible function arguments (the mesh / fe_engine argument \
                 given more than once)",
            ));
        }
    };
    if needs_fe && target.cast::<super::fe::PyFeEngine>().is_err() {
        return Err(PyTypeError::new_err(
            "initialize(): incompatible function arguments (positional arguments after the \
             first and `fe_engine=` need an FEEngine; a Mesh takes keyword arguments only)",
        ));
    }
    let extra = (1..n)
        .map(|i| args.get_item(i))
        .collect::<PyResult<Vec<_>>>()?;
    Ok((target, extra))
}

/// One argument of `initialize`: from position `$i` of the arguments after
/// the first, or from its keyword, or its default (both: `TypeError`).
macro_rules! init_arg {
    ($extra:ident, $i:expr, $kw:ident, $name:literal, $ty:ty, $default:expr) => {
        match ($extra.get($i), $kw) {
            (Some(_), Some(_)) => {
                return Err(pyo3::exceptions::PyTypeError::new_err(concat!(
                    "initialize() got multiple values for argument '",
                    $name,
                    "'"
                )));
            }
            (Some(v), None) => v.extract::<$ty>()?,
            (None, Some(v)) => v,
            (None, None) => $default,
        }
    };
}

macro_rules! etma_class {
    ($rust:ident, $pyname:literal, $t:ty, $default:expr) => {
        #[doc = concat!("`", $pyname, "` (PY-055).")]
        #[pyclass(subclass, frozen, name = $pyname, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust {
            pub(crate) provider: Arc<dyn EtmaProvider<$t>>,
        }

        impl $rust {
            /// Refer to mesh-owned arrays.
            #[allow(dead_code)] // macro-generated for every dtype; bool maps have no mesh source yet
            pub(crate) fn from_source(src: EtmaSource) -> Self {
                Self {
                    provider: Arc::new(MeshProvider(src)),
                }
            }

            /// Wrap any provider.
            #[allow(dead_code)] // used by model bindings for some dtypes only
            pub(crate) fn from_provider(p: Arc<dyn EtmaProvider<$t>>) -> Self {
                Self { provider: p }
            }
        }

        #[pymethods]
        impl $rust {
            #[new]
            #[pyo3(signature = (id = "by_element_type_array".to_owned(), parent_id = "no_parent".to_owned()))]
            fn new(id: String, parent_id: String) -> Self {
                Self {
                    provider: Arc::new(OwnProvider(Arc::new(SharedOwner::new(OwnEtma {
                        id: format!("{parent_id}:{id}"),
                        arrays: BTreeMap::new(),
                    })))),
                }
            }

            /// `self(type, ghost_type=_not_ghost)`: view of one block.
            #[pyo3(signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
            fn __call__<'py>(
                &self,
                py: Python<'py>,
                r#type: EtArg,
                ghost_type: GhostArg,
            ) -> PyResult<Bound<'py, PyAny>> {
                self.provider.call(py, r#type.0, ghost_type.single()?)
            }

            /// `elementTypes(dim=_all_dimensions, ghost_type=_not_ghost,
            /// kind=_ek_regular)`.
            #[pyo3(name = "elementTypes", signature = (dim = PY_ALL_DIMENSIONS, ghost_type = GhostArg(Some(GhostType::NotGhost)), kind = KindArg(Some(akantu::core::ElementKind::Regular))))]
            fn element_types<'py>(
                &self,
                py: Python<'py>,
                dim: i64,
                ghost_type: GhostArg,
                kind: KindArg,
            ) -> PyResult<Vec<Bound<'py, PyAny>>> {
                let filter = TypeFilter {
                    spatial_dimension: dim_filter(dim)?,
                    ghost_type: ghost_type.single()?,
                    kind: kind.0,
                };
                self.provider
                    .types(py, filter)?
                    .into_iter()
                    .map(|t| Ok(element_type_obj(py, t)?.into_any()))
                    .collect()
            }

            /// `initialize(…)`, upstream's three overloads
            /// (`py_mesh.cc:73-140`), resolved in upstream's order:
            ///
            /// 1. `initialize(mesh, *, ghost_type=_casper, nb_component=1,
            ///    spatial_dimension=-2, element_kind=_ek_not_defined,
            ///    with_nb_element=False, with_nb_nodes_per_element=False,
            ///    default_value=T(), do_not_default=False)` with a `Mesh`
            ///    (`spatial_dimension=-2` = the mesh's);
            /// 2. the same keyword-only form with an `FEEngine` passed as
            ///    `mesh`;
            /// 3. `initialize(fe_engine, ghost_type, nb_component,
            ///    spatial_dimension, element_kind, with_nb_element,
            ///    with_nb_nodes_per_element, default_value, do_not_default)`
            ///    with an `FEEngine`, every argument positional or keyword
            ///    (`fe_engine=`, oracle probe).
            ///
            /// A `Mesh` with more than one positional argument, or a `Mesh`
            /// passed as `fe_engine`, matches no overload: `TypeError`, as
            /// upstream. The FEEngine forms use the engine's element types,
            /// one row per integration point of every element (upstream's
            /// `FEEngineElementTypeMapArrayInitializer` ignores
            /// `spatial_dimension`, `with_nb_element` and
            /// `with_nb_nodes_per_element`). Owned maps only.
            #[pyo3(signature = (*args, mesh = None, fe_engine = None, ghost_type = None, nb_component = None, spatial_dimension = None, element_kind = None, with_nb_element = None, with_nb_nodes_per_element = None, default_value = None, do_not_default = None))]
            #[allow(clippy::too_many_arguments)] // upstream named-argument list
            fn initialize(
                &self,
                py: Python<'_>,
                args: &Bound<'_, pyo3::types::PyTuple>,
                mesh: Option<Bound<'_, PyAny>>,
                fe_engine: Option<Bound<'_, PyAny>>,
                ghost_type: Option<GhostArg>,
                nb_component: Option<usize>,
                spatial_dimension: Option<i64>,
                element_kind: Option<KindArg>,
                with_nb_element: Option<bool>,
                with_nb_nodes_per_element: Option<bool>,
                default_value: Option<$t>,
                do_not_default: Option<bool>,
            ) -> PyResult<()> {
                let (target, extra) = initialize_target(args, mesh, fe_engine)?;
                let ghost_type = init_arg!(extra, 0, ghost_type, "ghost_type", GhostArg, GhostArg(None));
                let nb_component = init_arg!(extra, 1, nb_component, "nb_component", usize, 1);
                let spatial_dimension =
                    init_arg!(extra, 2, spatial_dimension, "spatial_dimension", i64, -2);
                let element_kind = init_arg!(extra, 3, element_kind, "element_kind", KindArg, KindArg(None));
                let with_nb_element =
                    init_arg!(extra, 4, with_nb_element, "with_nb_element", bool, false);
                let with_nb_nodes_per_element = init_arg!(
                    extra,
                    5,
                    with_nb_nodes_per_element,
                    "with_nb_nodes_per_element",
                    bool,
                    false
                );
                let default_value = init_arg!(extra, 6, default_value, "default_value", $t, $default);
                let do_not_default = init_arg!(extra, 7, do_not_default, "do_not_default", bool, false);
                let own = self.provider.owned().ok_or_else(|| {
                    super::errors::exception(
                        py,
                        "initialize() is only available on an ElementTypeMapArray created from Python"
                            .into(),
                    )
                })?;
                if let Ok(fe) = target.cast::<super::fe::PyFeEngine>() {
                    let blocks = super::fe::quadrature_blocks(
                        py,
                        fe.get(),
                        ghost_type.0,
                        nb_component,
                        element_kind.0,
                    )?;
                    let fill = default_value;
                    return write_blocks(py, Arc::clone(own), blocks, fill, do_not_default);
                }
                let mesh = target.cast::<PyMesh>().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "initialize(mesh | fe_engine, …): expected a Mesh or an FEEngine",
                    )
                })?;
                initialize_owned(
                    py,
                    own,
                    &PyMesh::mesh_ref(mesh),
                    ghost_type.0,
                    nb_component,
                    spatial_dimension,
                    element_kind,
                    with_nb_element,
                    with_nb_nodes_per_element,
                    default_value,
                    do_not_default,
                )
            }

            #[pyo3(name = "getID")]
            fn get_id(&self, py: Python<'_>) -> PyResult<String> {
                self.provider.id(py)
            }
        }
    };
}

etma_class!(EtmaReal, "ElementTypeMapArrayReal", f64, 0.0);

/// A Python-owned `ElementTypeMapArrayReal` holding `blocks` (of one ghost
/// type), e.g. the result of `flattenInternalReal`.
pub(crate) fn owned_real(
    id: &str,
    ghost: GhostType,
    blocks: Vec<(ElementType, Array<f64>)>,
) -> EtmaReal {
    let mut own = OwnEtma {
        id: id.to_owned(),
        arrays: BTreeMap::new(),
    };
    for (ty, mut a) in blocks {
        a.set_id(format!(
            "{id}:{ty}{}",
            if ghost == GhostType::Ghost {
                ":ghost"
            } else {
                ""
            }
        ));
        own.arrays.insert((ghost, ty), SharedArray::new(a));
    }
    EtmaReal {
        provider: Arc::new(OwnProvider(Arc::new(SharedOwner::new(own)))),
    }
}
etma_class!(EtmaUInt, "ElementTypeMapArrayUInt", u32, 0);
etma_class!(EtmaInt, "ElementTypeMapArrayInt", i32, 0);
etma_class!(EtmaBool, "ElementTypeMapArraybool", bool, false);

/// Register the four classes.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<EtmaReal>()?;
    m.add_class::<EtmaUInt>()?;
    m.add_class::<EtmaInt>()?;
    m.add_class::<EtmaBool>()?;
    Ok(())
}
