// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_parser.cc (ParameterRegistry,
// Parsable, ParserParameter, ParserSection, Parser, parseInput,
// getStaticParser).

//! Parser bindings (PY-031, PY-040…043).
//!
//! * `ParameterRegistry` / `Parsable`: runtime-registered parameters owned
//!   by the Python object ([`DynamicParameters`], not upstream's leaked
//!   global map — UD-022, inventory A.9 #4). Reading a parameter registered
//!   without default before it is set raises (upstream returns
//!   uninitialised memory).
//! * `ParserSection` / `ParserParameter` objects are *paths* into a parser
//!   tree (the global parser of `parseInput`, or a parser/section created
//!   from Python); every call re-resolves the path, so an object kept across
//!   a later `parseInput` refers to the section at the same position of the
//!   new tree, or raises if there is none (upstream: dangling reference).
//! * Strings: names and values are returned as `str` when they are valid
//!   UTF-8, else as `bytes` (the byte-preserving parser keeps every byte,
//!   `docs/ARCHITECTURE.md`); `str` arguments are UTF-8 encoded.

use std::sync::{Arc, Mutex, PoisonError};

use akantu::core::{Error, Result};
use akantu::model::params::DynamicParameters;
use akantu::parser::param::{
    ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry, Parsable,
    RandomDistribution, RandomParameter,
};
use akantu::parser::{ByteString, Parser, ParserSection, ParserType, SearchScope};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyFloat, PyInt, PyString};

use super::arrays::{input_f64, input_matrix, matrix_to_numpy, vector_to_numpy};
use super::enums::{DistributionArg, ParserTypeArg};
use super::runtime::{IntoPy, native};
use crate::foreign_view::SharedOwner;

// --- strings ----------------------------------------------------------------

/// `str` if valid UTF-8, else `bytes`.
pub(crate) fn bytes_to_py<'py>(py: Python<'py>, b: &[u8]) -> Bound<'py, PyAny> {
    match std::str::from_utf8(b) {
        Ok(s) => PyString::new(py, s).into_any(),
        Err(_) => PyBytes::new(py, b).into_any(),
    }
}

fn name_arg(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(s) = obj.cast::<PyString>() {
        Ok(s.to_cow()?.as_bytes().to_vec())
    } else if let Ok(b) = obj.cast::<PyBytes>() {
        Ok(b.as_bytes().to_vec())
    } else {
        Err(PyTypeError::new_err("expected str or bytes"))
    }
}

// --- parser trees -------------------------------------------------------------

#[derive(Clone, Debug)]
enum Root {
    /// The process-global parser (`getStaticParser()`, filled by
    /// `parseInput`).
    Global,
    /// A parser created from Python (`Parser()`).
    Parser(Arc<Mutex<Parser>>),
    /// A standalone section created from Python (`ParserSection(...)`).
    Section(Arc<Mutex<ParserSection>>),
}

/// A section designated by its root and the indices of the sub-sections
/// leading to it.
#[derive(Clone, Debug)]
pub(crate) struct SectionPath {
    root: Root,
    path: Vec<usize>,
}

fn walk<'s>(mut s: &'s ParserSection, path: &[usize]) -> Result<&'s ParserSection> {
    for &i in path {
        s = s.sub_sections.get(i).ok_or_else(|| {
            Error::invalid("the parser section no longer exists (the input was parsed again)")
        })?;
    }
    Ok(s)
}

impl SectionPath {
    /// Run `f` on the designated section (and the permissive flag of its
    /// parser).
    pub(crate) fn with<R>(&self, f: impl FnOnce(&ParserSection, bool) -> Result<R>) -> Result<R> {
        match &self.root {
            Root::Global => {
                let rt = akantu::runtime::global()
                    .read()
                    .unwrap_or_else(PoisonError::into_inner);
                f(
                    walk(&rt.parser.root, &self.path)?,
                    rt.parser.is_permissive(),
                )
            }
            Root::Parser(p) => {
                let p = p.lock().unwrap_or_else(PoisonError::into_inner);
                f(walk(&p.root, &self.path)?, p.is_permissive())
            }
            Root::Section(s) => {
                let s = s.lock().unwrap_or_else(PoisonError::into_inner);
                f(walk(&s, &self.path)?, global_permissive())
            }
        }
    }

    fn child(&self, index: usize) -> Self {
        let mut path = self.path.clone();
        path.push(index);
        Self {
            root: self.root.clone(),
            path,
        }
    }
}

fn global_permissive() -> bool {
    akantu::runtime::global()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .parser
        .is_permissive()
}

/// `ParserSection` (PY-043).
#[pyclass(
    subclass,
    frozen,
    name = "ParserSection",
    module = "akantu.py11_akantu"
)]
#[derive(Debug)]
pub(crate) struct PyParserSection {
    pub(crate) path: SectionPath,
}

#[pymethods]
impl PyParserSection {
    /// `ParserSection(name, type)` or `ParserSection(name, type, option,
    /// parent)` (the parent's parameters become the variable scope).
    #[new]
    #[pyo3(signature = (name, r#type, option = None, parent = None))]
    fn new(
        name: String,
        r#type: ParserTypeArg,
        option: Option<String>,
        parent: Option<&Bound<'_, PyParserSection>>,
    ) -> PyResult<Self> {
        let section = ParserSection::with_option(r#type.0, name, option.unwrap_or_default());
        let py = match parent {
            Some(p) => p.py(),
            None => {
                return Ok(Self {
                    path: SectionPath {
                        root: Root::Section(Arc::new(Mutex::new(section))),
                        path: Vec::new(),
                    },
                });
            }
        };
        let parent = parent.map(|p| p.get().path.clone());
        let parent = parent.unwrap_or(SectionPath {
            root: Root::Global,
            path: Vec::new(),
        });
        // Upstream links the new section to its parent scope without adding
        // it to the parent; here the section is built as a child of a copy
        // of the parent, so the parent's parameters (and ancestors) are in
        // scope while the parent itself is unchanged.
        let built = parent
            .with(|p, _| {
                let mut scope = p.clone();
                scope.sub_sections.clear();
                scope.add_sub_section(section);
                Ok(scope)
            })
            .py_err(py)?;
        Ok(Self {
            path: SectionPath {
                root: Root::Section(Arc::new(Mutex::new(built))),
                path: vec![0],
            },
        })
    }

    /// `getParameter(name)`: parameter of this section (current scope, as
    /// upstream's default).
    #[pyo3(name = "getParameter")]
    fn get_parameter(
        &self,
        py: Python<'_>,
        name: &Bound<'_, PyAny>,
    ) -> PyResult<PyParserParameter> {
        let name = name_arg(name)?;
        self.path
            .with(|s, _| s.get_parameter(&name, SearchScope::Current).map(|_| ()))
            .py_err(py)?;
        Ok(PyParserParameter {
            section: self.path.clone(),
            name,
        })
    }

    /// `getSubSections(type, id=0)`: the `id`-th sub-section of `type`.
    #[pyo3(name = "getSubSections", signature = (r#type, id = 0))]
    fn get_sub_sections(&self, py: Python<'_>, r#type: ParserTypeArg, id: usize) -> PyResult<Self> {
        let index = self
            .path
            .with(|s, _| {
                s.sub_sections
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.section_type == r#type.0)
                    .nth(id)
                    .map(|(i, _)| i)
                    .ok_or_else(|| Error::invalid("Sub section not found"))
            })
            .py_err(py)?;
        Ok(Self {
            path: self.path.child(index),
        })
    }

    /// Section name (port extension, upstream `getName`).
    #[pyo3(name = "getName")]
    fn get_name(&self, py: Python<'_>) -> PyResult<String> {
        self.path.with(|s, _| Ok(s.name.clone())).py_err(py)
    }

    /// Option word (port extension, upstream `getOption`).
    #[pyo3(name = "getOption")]
    fn get_option(&self, py: Python<'_>) -> PyResult<String> {
        self.path.with(|s, _| Ok(s.option.clone())).py_err(py)
    }

    fn __repr__(&self) -> String {
        self.path
            .with(|s, _| Ok(format!("<ParserSection {} {}>", s.section_type, s.name)))
            .unwrap_or_else(|_| "<ParserSection (gone)>".into())
    }
}

/// `Parser` (PY-043): a `ParserSection` that is the root of a parsed file.
#[pyclass(extends = PyParserSection, frozen, name = "Parser", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyParser;

#[pymethods]
impl PyParser {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyParserSection {
            path: SectionPath {
                root: Root::Parser(Arc::new(Mutex::new(Parser::default()))),
                path: Vec::new(),
            },
        })
        .add_subclass(PyParser)
    }
}

/// `ParserParameter` (PY-042).
#[pyclass(frozen, name = "ParserParameter", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyParserParameter {
    section: SectionPath,
    name: Vec<u8>,
}

impl PyParserParameter {
    fn with<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(akantu::parser::section::ParamRef<'_>) -> Result<R>,
    ) -> PyResult<R> {
        self.section
            .with(|s, _| f(s.get_parameter(&self.name, SearchScope::Current)?))
            .py_err(py)
    }
}

#[pymethods]
impl PyParserParameter {
    #[pyo3(name = "getName")]
    fn get_name<'py>(&self, py: Python<'py>) -> Bound<'py, PyAny> {
        bytes_to_py(py, &self.name)
    }

    /// Raw value text (`"2*pi + max(2, general)"`).
    #[pyo3(name = "getValue")]
    fn get_value<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = self.with(py, |p| Ok(p.value().as_bytes().to_vec()))?;
        Ok(bytes_to_py(py, &v))
    }

    /// `Int` conversion (stream extraction: never fails, `"abc"` → 0).
    #[pyo3(name = "asInt")]
    fn as_int(&self, py: Python<'_>) -> PyResult<i32> {
        self.with(py, |p| Ok(p.to_int::<i32>()))
    }

    /// `Real` conversion (algebraic expression in the section's scope).
    #[pyo3(name = "asReal")]
    fn as_real(&self, py: Python<'_>) -> PyResult<f64> {
        self.with(py, |p| p.to_real())
    }

    #[pyo3(name = "asBool")]
    fn as_bool(&self, py: Python<'_>) -> PyResult<bool> {
        self.with(py, |p| Ok(p.to_bool()))
    }

    /// `Vector<Real>` conversion (copy, shape `(n,)`).
    #[pyo3(name = "asVector")]
    fn as_vector<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = self.with(py, |p| p.to_vector())?;
        vector_to_numpy(py, v)
    }

    /// `Matrix<Real>` conversion (copy, Fortran-ordered `(r, c)`; upstream
    /// returned a reference to a temporary — inventory A.9 #5).
    #[pyo3(name = "asMatrix")]
    fn as_matrix<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let m = self.with(py, |p| p.to_matrix())?;
        matrix_to_numpy(py, &m)
    }

    fn __repr__(&self) -> String {
        format!("<ParserParameter {}>", String::from_utf8_lossy(&self.name))
    }
}

/// `parseInput(file)`: parse into the global parser (PY-031). As upstream
/// the global tree is replaced by the file's content.
#[pyfunction]
#[pyo3(name = "parseInput")]
fn parse_input(py: Python<'_>, input_file: std::path::PathBuf) -> PyResult<()> {
    akantu::runtime::parse_input(&input_file).py_err(py)
}

/// `getStaticParser()`: the global parser.
#[pyfunction]
#[pyo3(name = "getStaticParser")]
fn get_static_parser(py: Python<'_>) -> PyResult<Bound<'_, PyParser>> {
    Bound::new(
        py,
        PyClassInitializer::from(PyParserSection {
            path: SectionPath {
                root: Root::Global,
                path: Vec::new(),
            },
        })
        .add_subclass(PyParser),
    )
}

// --- parameter registries -----------------------------------------------------

/// Native state of a Python `ParameterRegistry`/`Parsable`.
#[derive(Debug)]
pub(crate) struct RegistryCore {
    section_type: ParserType,
    id: String,
    params: DynamicParameters,
}

impl ParameterRegistry for RegistryCore {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.params.get(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.params.set(name, value)
    }
    /// Runtime names are not `'static`.
    fn param_names(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.params.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.params.set_from_input(name, value)
    }
}

impl Parsable for RegistryCore {
    fn section_type(&self) -> ParserType {
        self.section_type
    }
    fn parsable_id(&self) -> &str {
        &self.id
    }
}

/// `ParameterRegistry` (PY-040). Not constructible (as upstream).
#[pyclass(
    subclass,
    frozen,
    name = "ParameterRegistry",
    module = "akantu.py11_akantu"
)]
#[derive(Debug)]
pub(crate) struct PyParameterRegistry {
    pub(crate) core: RegistryRef,
}

/// What [`RegistryAccess::prepare`] admitted for one binding call: the
/// state lent to a running callback of a Python law, if the caller runs
/// inside that callback; empty for other registries.
#[derive(Clone, Debug, Default)]
pub(crate) struct Prepared(
    pub(crate) Option<super::laws::AdmittedLent>,
    /// The state lent to a running callback of a Python phase field,
    /// admitted for the caller.
    pub(crate)  Option<(
        crate::scoped::ScopedMut<akantu::phase_field::ForeignPhaseFieldState>,
        crate::scoped::ContextKey,
    )>,
);

impl Prepared {
    /// A use of the state lent to a phase-field callback.
    pub(crate) fn pf(
        st: crate::scoped::ScopedMut<akantu::phase_field::ForeignPhaseFieldState>,
        key: crate::scoped::ContextKey,
    ) -> Self {
        Self(None, Some((st, key)))
    }

    /// The phase-field lend of this use, if any.
    pub(crate) fn pf_lent(
        &self,
    ) -> Option<(
        crate::scoped::ScopedMut<akantu::phase_field::ForeignPhaseFieldState>,
        crate::scoped::ContextKey,
    )> {
        self.1.clone()
    }
}

/// Access to a registry owned by a Rust object (a material or solver of a
/// model): `f` runs inside the caller's native section with the owner
/// borrowed.
pub(crate) trait RegistryAccess: Send + Sync + std::fmt::Debug {
    /// Prepare one use by the calling Python code, before its native
    /// section (where no Python frame may be looked at).
    fn prepare(&self, py: Python<'_>) -> Result<Prepared> {
        let _ = py;
        Ok(Prepared::default())
    }

    /// Run `f` on the registry.
    fn with_registry(
        &self,
        prepared: &Prepared,
        f: &mut dyn FnMut(&mut dyn ParameterRegistry) -> Result<()>,
    ) -> Result<()>;

    /// `registerParamReal` on the owner (runtime laws accept it).
    fn register_param_real(
        &self,
        prepared: &Prepared,
        name: &str,
        access: ParameterAccess,
        description: &str,
        default: Option<f64>,
    ) -> Result<()> {
        let _ = (prepared, name, access, description, default);
        Err(Error::Unsupported(
            "registering parameters on a registry owned by akantu".into(),
        ))
    }
}

/// The registry behind a Python `ParameterRegistry`.
#[derive(Clone, Debug)]
pub(crate) enum RegistryRef {
    /// Owned by the Python object (`Parsable(...)` created from Python).
    Own(Arc<SharedOwner<RegistryCore>>),
    /// Owned by a Rust object (`model.getMaterial(0)`,
    /// `model.getNonLinearSolver()`).
    External(Arc<dyn RegistryAccess>),
}

impl RegistryRef {
    /// Prepare one use by the calling Python code (before its native
    /// section).
    pub(crate) fn prepare(&self, py: Python<'_>) -> Result<Prepared> {
        match self {
            Self::Own(_) => Ok(Prepared::default()),
            Self::External(access) => access.prepare(py),
        }
    }

    /// Run `f` on the registry (native section required; `prepared` from
    /// [`Self::prepare`] for this call).
    pub(crate) fn with<R>(
        &self,
        prepared: &Prepared,
        f: impl FnOnce(&mut dyn ParameterRegistry) -> Result<R>,
    ) -> Result<R> {
        match self {
            Self::Own(core) => {
                let mut c = core.try_borrow_mut()?;
                f(&mut *c)
            }
            Self::External(access) => {
                let mut f = Some(f);
                let mut out = None;
                access.with_registry(prepared, &mut |r| {
                    let f = f.take().ok_or_else(|| {
                        Error::invalid("internal error: registry callback reused")
                    })?;
                    out = Some(f(r)?);
                    Ok(())
                })?;
                out.ok_or_else(|| Error::invalid("internal error: registry callback not run"))
            }
        }
    }

    /// The owned core (Python-defined registries only).
    fn own(&self) -> Result<&Arc<SharedOwner<RegistryCore>>> {
        match self {
            Self::Own(c) => Ok(c),
            Self::External(_) => Err(Error::Unsupported(
                "registering parameters on a registry owned by akantu".into(),
            )),
        }
    }
}

fn access_from(py: Python<'_>, v: i64) -> PyResult<ParameterAccess> {
    u16::try_from(v)
        .map(ParameterAccess)
        .map_err(|_| super::errors::exception(py, format!("invalid parameter access type {v}")))
}

/// Python value → parameter value (upstream `ParsableVariant`:
/// `Real | Int | bool | str | Matrix | Vector`).
pub(crate) fn param_value(obj: &Bound<'_, PyAny>) -> PyResult<ParamValue> {
    if let Some(name) = super::enums::enum_member_name(obj)? {
        // akantu enum instance (`SolveConvergenceCriteria.residual`): the
        // member spelling, parsed by enumeration parameters.
        return Ok(ParamValue::Enum(name));
    }
    if obj.is_instance_of::<PyBool>() {
        return Ok(ParamValue::Bool(obj.extract()?));
    }
    if obj.is_instance_of::<PyInt>() {
        return Ok(ParamValue::Int(obj.extract()?));
    }
    if obj.is_instance_of::<PyFloat>() {
        return Ok(ParamValue::Real(obj.extract()?));
    }
    if let Ok(s) = obj.cast::<PyString>() {
        return Ok(ParamValue::String(s.to_cow()?.into_owned()));
    }
    if let Ok(b) = obj.cast::<PyBytes>() {
        return Ok(ParamValue::ByteString(ByteString::from(b.as_bytes())));
    }
    let (values, shape) = input_f64(obj)?;
    match shape.len() {
        0 => Ok(ParamValue::Real(values.first().copied().unwrap_or(0.0))),
        1 => Ok(ParamValue::VectorReal(values)),
        2 => Ok(ParamValue::MatrixReal(input_matrix(obj)?)),
        _ => Err(PyTypeError::new_err("unsupported parameter value")),
    }
}

/// Parameter value → Python.
fn value_to_py<'py>(py: Python<'py>, v: ParamValue) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        ParamValue::Real(x) => PyFloat::new(py, x).into_any(),
        ParamValue::Int(i) => PyInt::new(py, i).into_any(),
        ParamValue::Bool(b) => PyBool::new(py, b).to_owned().into_any(),
        ParamValue::String(s) | ParamValue::Enum(s) => PyString::new(py, &s).into_any(),
        ParamValue::ByteString(b) => bytes_to_py(py, b.as_bytes()),
        ParamValue::VectorReal(v) => vector_to_numpy(py, v)?,
        ParamValue::MatrixReal(m) => matrix_to_numpy(py, &m)?,
        ParamValue::Random(r) if r.distribution.is_none() => {
            PyFloat::new(py, r.base_value).into_any()
        }
        ParamValue::StringList(l) => pyo3::types::PyList::new(py, l)?.into_any(),
        ParamValue::StringSet(s) => pyo3::types::PySet::new(py, s)?.into_any(),
        ParamValue::ByteStringSet(s) => {
            let items: Vec<Bound<'py, PyAny>> =
                s.iter().map(|b| bytes_to_py(py, b.as_bytes())).collect();
            pyo3::types::PySet::new(py, items)?.into_any()
        }
        other => {
            return Err(super::errors::exception(
                py,
                format!(
                    "Parameter not of a known type or with wrong access rights ({:?})",
                    other.kind()
                ),
            ));
        }
    })
}

impl PyParameterRegistry {
    fn new_core(section_type: ParserType, id: String) -> Self {
        Self {
            core: RegistryRef::Own(Arc::new(SharedOwner::new(RegistryCore {
                section_type,
                id,
                params: DynamicParameters::new(),
            }))),
        }
    }

    /// A registry backed by a Rust-owned object.
    pub(crate) fn external(access: Arc<dyn RegistryAccess>) -> Self {
        Self {
            core: RegistryRef::External(access),
        }
    }

    pub(crate) fn get_value(&self, py: Python<'_>, name: &str) -> PyResult<ParamValue> {
        let core = self.core.clone();
        let name = name.to_owned();
        let prepared = core.prepare(py).py_err(py)?;
        native(py, move || core.with(&prepared, |r| r.get_param(&name)))
    }

    /// Upstream `ParameterRegistry::set`: set, then
    /// `updateInternalParameters()`.
    pub(crate) fn set_value(&self, py: Python<'_>, name: &str, v: ParamValue) -> PyResult<()> {
        let core = self.core.clone();
        let name = name.to_owned();
        let prepared = core.prepare(py).py_err(py)?;
        native(py, move || {
            core.with(&prepared, |r| {
                r.set_param(&name, v)?;
                r.update_internal_parameters()
            })
        })
    }
}

#[pymethods]
impl PyParameterRegistry {
    /// `registerParamReal(name, type, description)` or
    /// `registerParamReal(default, name, type, description)`.
    #[pyo3(name = "registerParamReal", signature = (*args))]
    fn register_param_real(
        &self,
        py: Python<'_>,
        args: &Bound<'_, pyo3::types::PyTuple>,
    ) -> PyResult<()> {
        let (default, name, access, description) = match args.len() {
            3 => (
                None,
                args.get_item(0)?.extract::<String>()?,
                args.get_item(1)?.extract::<i64>()?,
                args.get_item(2)?.extract::<String>()?,
            ),
            4 => (
                Some(args.get_item(0)?.extract::<f64>()?),
                args.get_item(1)?.extract::<String>()?,
                args.get_item(2)?.extract::<i64>()?,
                args.get_item(3)?.extract::<String>()?,
            ),
            _ => {
                return Err(PyTypeError::new_err(
                    "registerParamReal(name, type, description) or registerParamReal(default, name, type, description)",
                ));
            }
        };
        let access = access_from(py, access)?;
        let core = self.core.clone();
        let prepared = core.prepare(py).py_err(py)?;
        native(py, move || match &core {
            RegistryRef::External(a) => {
                a.register_param_real(&prepared, &name, access, &description, default)
            }
            RegistryRef::Own(_) => core.own()?.try_borrow_mut()?.params.register(
                &name,
                ParamKind::Real,
                access,
                &description,
                default.map(ParamValue::Real),
            ),
        })
    }

    /// `set(name, value)` (value: float, int, bool, str, vector, matrix).
    /// Integers are accepted for real parameters (upstream raises "Cannot
    /// cast std::any to parameter"; deviation: implicit conversion as for
    /// input files).
    fn set(&self, py: Python<'_>, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = param_value(value)?;
        self.set_value(py, name, v)
    }

    /// `get(name)` → float, int, bool, str, ndarray.
    ///
    /// As upstream (`py_parser.cc:88-110`), the generic getter reads the
    /// parameter through the **mutable** `Parameter::get<T>()`
    /// (`parameter_registry_tmpl.hh:132-138`), which requires the parameter
    /// to be readable **and writable**: a readable-only parameter (e.g. a
    /// material's derived `lambda`) raises `akantu.Exception("Parameter
    /// <name> is not accessible")`, while `getReal` / `getInt` / … (the
    /// const conversion) return it (PY-040; oracle probe).
    /// A parameter that is not readable raises upstream's "not of a
    /// known type or with wrong access rights".
    fn get<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        let owned = name.to_owned();
        let prepared = core.prepare(py).py_err(py)?;
        let info = native(py, move || {
            core.with(&prepared, |r| Ok(r.param_info(&owned)))
        })?;
        if let Some(info) = info {
            if !info.access.contains(ParameterAccess::READABLE) {
                return Err(super::errors::exception(
                    py,
                    "Paramter not of a known type or with wrong access rights".into(),
                ));
            }
            if !info.access.contains(ParameterAccess::WRITABLE) {
                return Err(super::errors::exception(
                    py,
                    format!("Parameter {name} is not accessible"),
                ));
            }
        }
        let v = self.get_value(py, name)?;
        value_to_py(py, v)
    }

    #[pyo3(name = "getReal")]
    fn get_real(&self, py: Python<'_>, name: &str) -> PyResult<f64> {
        self.get_value(py, name)?.as_real().py_err(py)
    }

    #[pyo3(name = "getBool")]
    fn get_bool(&self, py: Python<'_>, name: &str) -> PyResult<bool> {
        self.get_value(py, name)?.as_bool().py_err(py)
    }

    #[pyo3(name = "getString")]
    fn get_string<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        let v = self.get_value(py, name)?;
        let b = v.as_bytes().py_err(py)?;
        Ok(bytes_to_py(py, b))
    }

    #[pyo3(name = "getInt")]
    fn get_int(&self, py: Python<'_>, name: &str) -> PyResult<i64> {
        self.get_value(py, name)?.as_int().py_err(py)
    }

    /// `getMatrix(name)` (copy).
    #[pyo3(name = "getMatrix")]
    fn get_matrix<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        match self.get_value(py, name)? {
            ParamValue::MatrixReal(m) => matrix_to_numpy(py, &m),
            other => Err(super::errors::exception(
                py,
                format!(
                    "Parameter {name} type error, cannot convert {:?} to Matrix",
                    other.kind()
                ),
            )),
        }
    }

    #[pyo3(name = "setReal")]
    fn set_real(&self, py: Python<'_>, name: &str, value: f64) -> PyResult<()> {
        self.set_value(py, name, ParamValue::Real(value))
    }

    #[pyo3(name = "setBool")]
    fn set_bool(&self, py: Python<'_>, name: &str, value: bool) -> PyResult<()> {
        self.set_value(py, name, ParamValue::Bool(value))
    }

    #[pyo3(name = "setString")]
    fn set_string(&self, py: Python<'_>, name: &str, value: String) -> PyResult<()> {
        self.set_value(py, name, ParamValue::String(value))
    }

    #[pyo3(name = "setInt")]
    fn set_int(&self, py: Python<'_>, name: &str, value: i32) -> PyResult<()> {
        self.set_value(py, name, ParamValue::Int(i64::from(value)))
    }

    /// `setRandom(name, value, distribution, a, b)`. For a parameter of
    /// random kind the random parameter is set (inventory A.9 #3: upstream
    /// built it and then set the plain value); otherwise the plain `value`
    /// is set, as upstream.
    #[pyo3(name = "setRandom")]
    fn set_random(
        &self,
        py: Python<'_>,
        name: &str,
        value: f64,
        distribution: DistributionArg,
        a: f64,
        b: f64,
    ) -> PyResult<()> {
        let dist = match distribution.0 {
            0 => RandomDistribution::Uniform(a, b),
            1 => RandomDistribution::Exponential(a),
            2 => RandomDistribution::Gamma(a, b),
            3 => RandomDistribution::Weibull(a, b),
            4 => RandomDistribution::ExtremeValue(a, b),
            5 => RandomDistribution::Normal(a, b),
            6 => RandomDistribution::Lognormal(a, b),
            7 => RandomDistribution::ChiSquared(a),
            8 => RandomDistribution::Cauchy(a, b),
            9 => RandomDistribution::FisherF(a, b),
            10 => RandomDistribution::StudentT(a),
            v => {
                return Err(super::errors::exception(
                    py,
                    format!("invalid distribution {v}"),
                ));
            }
        };
        let core = self.core.clone();
        let name = name.to_owned();
        let prepared = core.prepare(py).py_err(py)?;
        native(py, move || {
            core.with(&prepared, |c| {
                let random = c
                    .param_info(&name)
                    .is_some_and(|i| i.kind == ParamKind::Random);
                let v = if random {
                    ParamValue::Random(RandomParameter {
                        base_value: value,
                        distribution: Some(dist),
                    })
                } else {
                    ParamValue::Real(value)
                };
                c.set_param(&name, v)?;
                c.update_internal_parameters()
            })
        })
    }
}

/// `Parsable(section_type, id)` (PY-041).
#[pyclass(extends = PyParameterRegistry, subclass, frozen, name = "Parsable", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyParsable;

#[pymethods]
impl PyParsable {
    #[new]
    #[pyo3(signature = (section_type, id = String::new()))]
    fn new(section_type: ParserTypeArg, id: String) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyParameterRegistry::new_core(section_type.0, id))
            .add_subclass(PyParsable)
    }

    /// `parseSection(section)`: set the parsable parameters from `section`
    /// (permissive mode of the section's parser applies).
    #[pyo3(name = "parseSection")]
    fn parse_section(slf: &Bound<'_, Self>, section: &Bound<'_, PyParserSection>) -> PyResult<()> {
        let py = slf.py();
        let core = slf.as_super().get().core.clone();
        let path = section.get().path.clone();
        native(py, move || {
            path.with(|s, permissive| core.own()?.try_borrow_mut()?.parse_section(s, permissive))
        })
    }
}

/// A Python-owned `Parsable(_model, id)` (the `Parsable` base of a model
/// object).
pub(crate) fn model_parsable(py: Python<'_>, id: String) -> PyResult<Py<PyAny>> {
    let init = PyClassInitializer::from(PyParameterRegistry::new_core(ParserType::Model, id))
        .add_subclass(PyParsable);
    Ok(Bound::new(py, init)?.into_any().unbind())
}

/// Register the parser bindings (`register_parser`).
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    super::enums::register_enum::<super::enums::ParameterAccessType>(m, true)?;
    m.add_class::<PyParameterRegistry>()?;
    m.add_class::<PyParsable>()?;
    m.add_class::<PyParserParameter>()?;
    m.add_class::<PyParserSection>()?;
    m.add_class::<PyParser>()?;
    m.add_function(wrap_pyfunction!(parse_input, m)?)?;
    m.add_function(wrap_pyfunction!(get_static_parser, m)?)?;
    Ok(())
}
