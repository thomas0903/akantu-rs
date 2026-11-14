// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_aka_common.cc:62-152
// (register_enums), python/py_aka_error.cc:37-50 (DebugLevel),
// python/py_parser.cc:45-52 (ParameterAccessType); values from
// src/common/aka_common.hh, aka_element_classes_info.hh, parser.hh,
// aka_error.hh, parameter_registry.hh, aka_random_generator.hh. Names and
// integer values were checked against the oracle module (PY-010…028).

//! Enumerations with pybind11 `py::enum_` semantics (PY-010…027).
//!
//! Each enum is a Python class (module `akantu.py11_akantu`) whose members
//! are class attributes **and** module attributes (`.export_values()`;
//! module-scope name collisions resolve "last registration wins", as
//! upstream: `aka.model` is `ParserType.model`, `aka.auto` is
//! `SparseSolverType.auto`, `aka.static` is `TimeStepSolverType.static`).
//! Instances support `int()`, `operator.index`, `hash`, `name`, `value`,
//! `__members__`, `repr` (`<SpatialDirection._x: 0>`), `str`
//! (`SpatialDirection._x`), construction from an integer and pickling.
//!
//! Equality follows pybind11: plain C enums (`_x`, `_static`, element types,
//! ghost types, …) compare equal to integers and to other enums with the same
//! value; class enums (`ModelType.model`, `ParserType.material`, …) compare
//! equal only to instances of the same class with the same value. Only the
//! arithmetic enum (`ParameterAccessType`) supports ordering and the bitwise
//! operators (which return `int`, so `_pat_readable | _pat_parsable` is
//! `4352`).
//!
//! Integer values are upstream's for the upstream wheel configuration
//! (cohesive and structural element types enabled, IGFEM disabled).

use pyo3::PyClass;
use pyo3::basic::CompareOp;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyType};

use akantu::core::{ElementKind, ElementType, GhostType};

/// Static description of one enum class.
pub(crate) trait EnumMeta: PyClass + Sized {
    /// Python class name.
    const NAME: &'static str;
    /// `(name, value)` in registration order (aliases allowed).
    const MEMBERS: &'static [(&'static str, i64)];
    /// Integer value of an instance.
    fn int_value(&self) -> i64;
    /// A new Python instance.
    fn instance(py: Python<'_>, value: i64) -> PyResult<Bound<'_, Self>>;
}

fn member_name<E: EnumMeta>(value: i64) -> &'static str {
    E::MEMBERS
        .iter()
        .find(|(_, v)| *v == value)
        .map_or("???", |(n, _)| n)
}

fn richcmp<'py, E: EnumMeta>(
    this: &Bound<'py, E>,
    other: &Bound<'py, PyAny>,
    op: CompareOp,
    convertible: bool,
    arithmetic: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let py = this.py();
    let value = this.borrow().int_value();
    let a = PyInt::new(py, value);
    let not_impl = || Ok(py.NotImplemented().into_bound(py));
    match op {
        CompareOp::Eq | CompareOp::Ne => {
            let eq = if convertible {
                !other.is_none() && a.as_any().eq(other)?
            } else {
                match other.cast::<E>() {
                    Ok(o) => o.borrow().int_value() == value,
                    Err(_) => false,
                }
            };
            let r = if matches!(op, CompareOp::Eq) { eq } else { !eq };
            Ok(pyo3::types::PyBool::new(py, r).to_owned().into_any())
        }
        _ if arithmetic && convertible => {
            let r = a.as_any().rich_compare(other, op)?;
            Ok(r)
        }
        _ => not_impl(),
    }
}

fn int_of(other: &Bound<'_, PyAny>) -> PyResult<i64> {
    other.extract::<i64>()
}

macro_rules! py_enum {
    (
        $(#[$meta:meta])*
        $rust:ident, $pyname:literal, convertible = $conv:literal, arithmetic = $arith:literal,
        [ $( $member:literal = $value:expr ),* $(,)? ]
    ) => {
        $(#[$meta])*
        #[pyclass(frozen, name = $pyname, module = "akantu.py11_akantu")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) struct $rust {
            pub(crate) value: i64,
        }

        impl EnumMeta for $rust {
            const NAME: &'static str = $pyname;
            const MEMBERS: &'static [(&'static str, i64)] = &[ $( ($member, $value) ),* ];
            fn int_value(&self) -> i64 {
                self.value
            }
            fn instance(py: Python<'_>, value: i64) -> PyResult<Bound<'_, Self>> {
                Bound::new(py, Self { value })
            }
        }

        #[pymethods]
        impl $rust {
            #[new]
            fn new(value: i64) -> Self {
                Self { value }
            }
            fn __int__(&self) -> i64 {
                self.value
            }
            fn __index__(&self) -> i64 {
                self.value
            }
            fn __hash__(&self) -> isize {
                isize::try_from(self.value).unwrap_or(isize::MAX)
            }
            fn __repr__(&self) -> String {
                format!("<{}.{}: {}>", $pyname, member_name::<Self>(self.value), self.value)
            }
            fn __str__(&self) -> String {
                format!("{}.{}", $pyname, member_name::<Self>(self.value))
            }
            #[getter]
            fn name(&self) -> &'static str {
                member_name::<Self>(self.value)
            }
            #[getter]
            fn value(&self) -> i64 {
                self.value
            }
            fn __richcmp__<'py>(
                slf: &Bound<'py, Self>,
                other: &Bound<'py, PyAny>,
                op: CompareOp,
            ) -> PyResult<Bound<'py, PyAny>> {
                richcmp::<Self>(slf, other, op, $conv, $arith)
            }
            fn __reduce__<'py>(slf: &Bound<'py, Self>) -> (Bound<'py, PyType>, (i64,)) {
                (slf.get_type(), (slf.get().value,))
            }
            fn __and__(&self, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
                arith_op($arith, other, |b| self.value & b)
            }
            fn __rand__(&self, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
                arith_op($arith, other, |b| b & self.value)
            }
            fn __or__(&self, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
                arith_op($arith, other, |b| self.value | b)
            }
            fn __ror__(&self, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
                arith_op($arith, other, |b| b | self.value)
            }
            fn __xor__(&self, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
                arith_op($arith, other, |b| self.value ^ b)
            }
            fn __rxor__(&self, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
                arith_op($arith, other, |b| b ^ self.value)
            }
            fn __invert__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
                if $arith {
                    Ok(PyInt::new(py, !self.value).into_any().unbind())
                } else {
                    Err(pyo3::exceptions::PyTypeError::new_err(concat!(
                        "bad operand type for unary ~: '", $pyname, "'"
                    )))
                }
            }
        }
    };
}

fn arith_op(
    arithmetic: bool,
    other: &Bound<'_, PyAny>,
    f: impl FnOnce(i64) -> i64,
) -> PyResult<Py<PyAny>> {
    let py = other.py();
    if !arithmetic {
        return Ok(py.NotImplemented());
    }
    match int_of(other) {
        Ok(b) => Ok(PyInt::new(py, f(b)).into_any().unbind()),
        Err(_) => Ok(py.NotImplemented()),
    }
}

py_enum!(
    /// `SpatialDirection` (PY-010).
    SpatialDirection, "SpatialDirection", convertible = true, arithmetic = false,
    ["_x" = 0, "_y" = 1, "_z" = 2]
);
py_enum!(
    /// `AnalysisMethod` (PY-011); `_explicit_lumped_capacity` aliases 2.
    AnalysisMethod, "AnalysisMethod", convertible = true, arithmetic = false,
    [
        "_static" = 0, "_implicit_dynamic" = 1, "_explicit_lumped_mass" = 2,
        "_explicit_lumped_capacity" = 2, "_explicit_consistent_mass" = 3,
        "_explicit_contact" = 4, "_implicit_contact" = 5,
    ]
);
py_enum!(
    /// `ModelType` (PY-012, class enum).
    ModelType, "ModelType", convertible = false, arithmetic = false,
    [
        "model" = 0, "solid_mechanics_model" = 1, "solid_mechanics_model_cohesive" = 2,
        "heat_transfer_model" = 3, "structural_mechanics_model" = 4, "embedded_model" = 5,
        "contact_mechanics_model" = 6, "coupler_solid_contact" = 7,
        "coupler_solid_cohesive_contact" = 8, "phase_field_model" = 9,
        "coupler_solid_phasefield" = 10,
    ]
);
py_enum!(
    /// `NonLinearSolverType` (PY-013, class enum).
    NonLinearSolverType, "NonLinearSolverType", convertible = false, arithmetic = false,
    [
        "linear" = 0, "newton_raphson" = 1, "newton_raphson_modified" = 2, "lumped" = 3,
        "newton_raphson_contact" = 4, "petsc_snes" = 5, "petsc_tao" = 6, "auto" = 7,
    ]
);
py_enum!(
    /// `TimeStepSolverType` (PY-014, class enum).
    TimeStepSolverType, "TimeStepSolverType", convertible = false, arithmetic = false,
    ["static" = 0, "dynamic" = 1, "dynamic_lumped" = 2, "not_defined" = 3]
);
py_enum!(
    /// `IntegrationSchemeType` (PY-015, class enum).
    IntegrationSchemeType, "IntegrationSchemeType", convertible = false, arithmetic = false,
    [
        "pseudo_time" = 0, "forward_euler" = 1, "trapezoidal_rule_1" = 2, "backward_euler" = 3,
        "central_difference" = 4, "fox_goodwin" = 5, "trapezoidal_rule_2" = 6,
        "linear_acceleration" = 7, "newmark_beta" = 8, "generalized_trapezoidal" = 9,
    ]
);
py_enum!(
    /// `SolveConvergenceCriteria` (PY-016, class enum).
    SolveConvergenceCriteria, "SolveConvergenceCriteria", convertible = false, arithmetic = false,
    ["residual" = 0, "solution" = 1, "residual_mass_wgh" = 2]
);
py_enum!(
    /// `SparseSolverType` (PY-017, class enum).
    SparseSolverType, "SparseSolverType", convertible = false, arithmetic = false,
    ["mumps" = 0, "eigen" = 1, "petsc" = 2, "auto" = 3]
);
py_enum!(
    /// `CohesiveMethod` (PY-018).
    CohesiveMethod, "CohesiveMethod", convertible = true, arithmetic = false,
    ["_intrinsic" = 0, "_extrinsic" = 1]
);
py_enum!(
    /// `GhostType` (PY-019); `_casper` = both ghost types.
    PyGhostType, "GhostType", convertible = true, arithmetic = false,
    ["_not_ghost" = 0, "_ghost" = 1, "_casper" = 2]
);
py_enum!(
    /// `MeshIOType` (PY-020).
    MeshIoType, "MeshIOType", convertible = true, arithmetic = false,
    [
        "_miot_auto" = 0, "_miot_gmsh" = 1, "_miot_gmsh_struct" = 2, "_miot_diana" = 3,
        "_miot_abaqus" = 4,
    ]
);
py_enum!(
    /// `MatrixType` (PY-021).
    MatrixType, "MatrixType", convertible = true, arithmetic = false,
    ["_unsymmetric" = 0, "_symmetric" = 1]
);
py_enum!(
    /// `RandomDistributionType.RandomDistributionTypes` (PY-022, sub-module).
    RandomDistributionTypes, "RandomDistributionTypes", convertible = false, arithmetic = false,
    [
        "_uniform" = 0, "_exponential" = 1, "_gamma" = 2, "_weibull" = 3,
        "_extreme_value" = 4, "_normal" = 5, "_lognormal" = 6, "_chi_squared" = 7,
        "_cauchy" = 8, "_fisher_f" = 9, "_student_t" = 10,
    ]
);
py_enum!(
    /// `ElementType` (PY-023). Values of the upstream wheel configuration
    /// (cohesive + structural); IGFEM types are not part of this module.
    PyElementType, "ElementType", convertible = true, arithmetic = false,
    [
        "_not_defined" = 0, "_point_1" = 1, "_segment_2" = 2, "_segment_3" = 3,
        "_triangle_3" = 4, "_triangle_6" = 5, "_quadrangle_4" = 6, "_quadrangle_8" = 7,
        "_tetrahedron_4" = 8, "_tetrahedron_10" = 9, "_pentahedron_6" = 10,
        "_pentahedron_15" = 11, "_hexahedron_8" = 12, "_hexahedron_20" = 13,
        "_cohesive_1d_2" = 14, "_cohesive_2d_4" = 15, "_cohesive_2d_6" = 16,
        "_cohesive_3d_12" = 17, "_cohesive_3d_16" = 18, "_cohesive_3d_6" = 19,
        "_cohesive_3d_8" = 20, "_bernoulli_beam_2" = 21, "_bernoulli_beam_3" = 22,
        "_discrete_kirchhoff_triangle_18" = 23,
    ]
);
py_enum!(
    /// `ElementKind` (PY-024).
    PyElementKind, "ElementKind", convertible = true, arithmetic = false,
    ["_ek_regular" = 0, "_ek_cohesive" = 1, "_ek_structural" = 2, "_ek_not_defined" = 3]
);
py_enum!(
    /// `ParserType` (PY-025, class enum).
    ParserType, "ParserType", convertible = false, arithmetic = false,
    [
        "cohesive_inserter" = 0, "contact" = 1, "embedded_interface" = 2, "friction" = 3,
        "global" = 4, "heat" = 5, "integration_scheme" = 6, "material" = 7,
        "phasefield" = 8, "constitutive_law" = 9, "mesh" = 10, "model" = 11,
        "model_solver" = 12, "neighborhood" = 13, "neighborhoods" = 14,
        "non_linear_solver" = 15, "non_local" = 16, "rules" = 17, "solver" = 18,
        "time_step_solver" = 19, "user" = 20, "weight_function" = 21,
        "contact_detector" = 22, "contact_resolution" = 23, "not_defined" = 24,
    ]
);
py_enum!(
    /// `DebugLevel` (PY-026).
    DebugLevel, "DebugLevel", convertible = true, arithmetic = false,
    [
        "dblError" = 0, "dblException" = 1, "dblCritical" = 1, "dblMajor" = 2,
        "dblWarning" = 4, "dblInfo" = 5, "dblTrace" = 8, "dblAccessory" = 9,
        "dblDebug" = 42, "dblDump" = 100, "dblTest" = 1337,
    ]
);
py_enum!(
    /// `ParameterAccessType` (PY-027, `py::arithmetic()`).
    ParameterAccessType, "ParameterAccessType", convertible = true, arithmetic = true,
    [
        "_pat_internal" = 0x0001, "_pat_writable" = 0x0010, "_pat_readable" = 0x0100,
        "_pat_modifiable" = 0x0110, "_pat_parsable" = 0x1000, "_pat_parsmod" = 0x1110,
    ]
);

/// Register `E` in `scope` (module or sub-module): class attributes,
/// `__members__`, `__doc__` and — with `export` — module attributes.
pub(crate) fn register_enum<E: EnumMeta>(
    scope: &Bound<'_, PyModule>,
    export: bool,
) -> PyResult<()> {
    let py = scope.py();
    scope.add_class::<E>()?;
    let ty = py.get_type::<E>();
    let members = PyDict::new(py);
    let mut doc = String::from("Members:\n\n");
    for (name, value) in E::MEMBERS {
        let inst = E::instance(py, *value)?;
        ty.setattr(*name, &inst)?;
        members.set_item(*name, &inst)?;
        if export {
            scope.setattr(*name, &inst)?;
        }
        doc.push_str("  ");
        doc.push_str(name);
        doc.push_str("\n\n");
    }
    ty.setattr("__members__", members)?;
    ty.setattr("__doc__", doc.trim_end())?;
    Ok(())
}

/// Register all enums in upstream order (`register_enums`).
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    register_enum::<SpatialDirection>(m, true)?;
    register_enum::<AnalysisMethod>(m, true)?;
    register_enum::<ModelType>(m, true)?;
    register_enum::<NonLinearSolverType>(m, true)?;
    register_enum::<TimeStepSolverType>(m, true)?;
    register_enum::<IntegrationSchemeType>(m, true)?;
    register_enum::<SolveConvergenceCriteria>(m, true)?;
    register_enum::<SparseSolverType>(m, true)?;
    register_enum::<CohesiveMethod>(m, true)?;
    register_enum::<PyGhostType>(m, true)?;
    register_enum::<MeshIoType>(m, true)?;
    register_enum::<MatrixType>(m, true)?;
    let rdt = PyModule::new(m.py(), "RandomDistributionType")?;
    register_enum::<RandomDistributionTypes>(&rdt, true)?;
    m.add_submodule(&rdt)?;
    register_enum::<PyElementType>(m, true)?;
    register_enum::<PyElementKind>(m, true)?;
    register_enum::<ParserType>(m, true)?;
    Ok(())
}

// --- argument conversions ---------------------------------------------------

/// Member name of an akantu enum instance (`None` for other objects).
pub(crate) fn enum_member_name(obj: &Bound<'_, PyAny>) -> PyResult<Option<String>> {
    let ty = obj.get_type();
    if ty
        .getattr("__module__")?
        .extract::<String>()
        .ok()
        .as_deref()
        != Some("akantu.py11_akantu")
        || !ty.hasattr("__members__")?
    {
        return Ok(None);
    }
    Ok(Some(obj.getattr("name")?.extract()?))
}

/// An enum argument converted by member name to the Rust enum `R`
/// (`FromStr` ignores a leading underscore); `None` for the members
/// meaning "default" (`not_defined`, `auto`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct NamedEnumArg<E, R>(pub Option<R>, std::marker::PhantomData<E>);

impl<'a, 'py, E: EnumMeta, R: std::str::FromStr<Err = akantu::core::Error>> FromPyObject<'a, 'py>
    for NamedEnumArg<E, R>
{
    type Error = PyErr;
    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        let v = value_of::<E>(&obj, <E as EnumMeta>::NAME)?;
        let name = member_name::<E>(v);
        if matches!(name, "not_defined" | "_not_defined" | "auto" | "_auto") {
            return Ok(Self(None, std::marker::PhantomData));
        }
        name.parse::<R>()
            .map(|r| Self(Some(r), std::marker::PhantomData))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

fn value_of<E: EnumMeta>(obj: &Bound<'_, PyAny>, what: &str) -> PyResult<i64> {
    match obj.cast::<E>() {
        Ok(e) => Ok(e.borrow().int_value()),
        Err(_) => Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "expected {} for {what}, got {}",
            <E as EnumMeta>::NAME,
            obj.get_type().name()?
        ))),
    }
}

/// An `ElementType` argument.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EtArg(pub ElementType);

impl<'a, 'py> FromPyObject<'a, 'py> for EtArg {
    type Error = PyErr;
    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        let v = value_of::<PyElementType>(&obj, "an element type")?;
        usize::try_from(v)
            .ok()
            .filter(|&i| i <= 23)
            .and_then(ElementType::from_index)
            .map(EtArg)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid ElementType value {v}"))
            })
    }
}

/// A `GhostType` argument (`None` = `_casper`, both ghost types).
#[derive(Clone, Copy, Debug)]
pub(crate) struct GhostArg(pub Option<GhostType>);

impl GhostArg {
    /// The ghost type, `_casper` rejected.
    pub(crate) fn single(self) -> PyResult<GhostType> {
        self.0.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(
                "_casper is not a ghost type of an element block here (use _not_ghost or _ghost)",
            )
        })
    }
}

impl<'a, 'py> FromPyObject<'a, 'py> for GhostArg {
    type Error = PyErr;
    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        match value_of::<PyGhostType>(&obj, "a ghost type")? {
            0 => Ok(Self(Some(GhostType::NotGhost))),
            1 => Ok(Self(Some(GhostType::Ghost))),
            2 => Ok(Self(None)),
            v => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "invalid GhostType value {v}"
            ))),
        }
    }
}

/// An `ElementKind` argument (`None` = `_ek_not_defined`, all kinds).
#[derive(Clone, Copy, Debug)]
pub(crate) struct KindArg(pub Option<ElementKind>);

impl<'a, 'py> FromPyObject<'a, 'py> for KindArg {
    type Error = PyErr;
    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        match value_of::<PyElementKind>(&obj, "an element kind")? {
            0 => Ok(Self(Some(ElementKind::Regular))),
            1 => Ok(Self(Some(ElementKind::Cohesive))),
            2 => Ok(Self(Some(ElementKind::Structural))),
            3 => Ok(Self(None)),
            v => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "invalid ElementKind value {v}"
            ))),
        }
    }
}

/// A `ParserType` argument.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ParserTypeArg(pub akantu::parser::ParserType);

impl<'a, 'py> FromPyObject<'a, 'py> for ParserTypeArg {
    type Error = PyErr;
    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        let v = value_of::<ParserType>(&obj, "a parser type")?;
        usize::try_from(v)
            .ok()
            .and_then(|i| akantu::parser::ParserType::ALL.get(i).copied())
            .map(ParserTypeArg)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid ParserType value {v}"))
            })
    }
}

/// A `MeshIOType` argument.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MeshIoArg(pub akantu::core::enums::MeshIoType);

impl<'a, 'py> FromPyObject<'a, 'py> for MeshIoArg {
    type Error = PyErr;
    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        let v = value_of::<MeshIoType>(&obj, "a mesh IO type")?;
        usize::try_from(v)
            .ok()
            .and_then(|i| akantu::core::enums::MeshIoType::ALL.get(i).copied())
            .map(MeshIoArg)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid MeshIOType value {v}"))
            })
    }
}

/// A `SpatialDirection` argument (axis index).
#[derive(Clone, Copy, Debug)]
pub(crate) struct DirectionArg(pub usize);

impl<'a, 'py> FromPyObject<'a, 'py> for DirectionArg {
    type Error = PyErr;
    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        let v = value_of::<SpatialDirection>(&obj, "a spatial direction")?;
        usize::try_from(v)
            .ok()
            .filter(|&d| d < 3)
            .map(DirectionArg)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "invalid SpatialDirection value {v}"
                ))
            })
    }
}

/// A `RandomDistributionTypes` argument.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DistributionArg(pub i64);

impl<'a, 'py> FromPyObject<'a, 'py> for DistributionArg {
    type Error = PyErr;
    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        value_of::<RandomDistributionTypes>(&obj, "a random distribution type").map(DistributionArg)
    }
}

/// Python instance of an element type.
pub(crate) fn element_type_obj(
    py: Python<'_>,
    t: ElementType,
) -> PyResult<Bound<'_, PyElementType>> {
    Bound::new(
        py,
        PyElementType {
            value: i64::try_from(t.index()).unwrap_or(0),
        },
    )
}

/// Python instance of a ghost type.
pub(crate) fn ghost_type_obj(py: Python<'_>, g: GhostType) -> PyResult<Bound<'_, PyGhostType>> {
    Bound::new(
        py,
        PyGhostType {
            value: i64::try_from(g.index()).unwrap_or(0),
        },
    )
}
