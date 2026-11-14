// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/parameter_registry.hh,
// parameter_registry_tmpl.hh, parameter_registry.cc, parsable.hh,
// parsable.cc, parser_real.cc (parseRandomParameter),
// src/common/aka_random_generator.hh (RandomParameter,
// make_random_parameter, printing).
// Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! Typed parameters, the parameter registry and `Parsable`.
//!
//! Upstream registers *references to member variables*
//! (`registerParam("E", E, …)`). Rust forbids such self-references, so a
//! registry is a static table of [`ParamDescriptor`]s holding plain accessor
//! function pointers `fn(&S) -> ParamValue` / `fn(&mut S, ParamValue)`
//! (decision record D-016). Tables are written once per type, e.g.
//!
//! ```
//! use akantu_parser::param::*;
//! #[derive(Debug, Default)]
//! struct Elastic { e: f64, nu: f64 }
//! static ELASTIC_PARAMS: &[ParamDescriptor<Elastic>] = &[
//!     ParamDescriptor::new("E", ParamKind::Real, ParameterAccess::PARSMOD, "Young's modulus",
//!         |s| ParamValue::Real(s.e), Some(|s, v| { s.e = v.as_real()?; Ok(()) })),
//!     ParamDescriptor::new("nu", ParamKind::Real, ParameterAccess::PARSMOD, "Poisson ratio",
//!         |s| ParamValue::Real(s.nu), Some(|s, v| { s.nu = v.as_real()?; Ok(()) })),
//! ];
//! impl ParameterRegistry for Elastic {
//!     fn get_param(&self, name: &str) -> akantu_core::Result<ParamValue> {
//!         table_get(ELASTIC_PARAMS, self, name)
//!     }
//!     fn set_param(&mut self, name: &str, v: ParamValue) -> akantu_core::Result<()> {
//!         table_set(ELASTIC_PARAMS, self, name, v)
//!     }
//!     fn param_names(&self) -> Vec<&'static str> { table_names(ELASTIC_PARAMS) }
//!     fn param_info(&self, name: &str) -> Option<ParamInfo> { table_info(ELASTIC_PARAMS, name) }
//!     fn set_param_from_input(&mut self, name: &str, v: ParamValue) -> akantu_core::Result<()> {
//!         table_set_from_input(ELASTIC_PARAMS, self, name, v)
//!     }
//! }
//! impl Parsable for Elastic {
//!     fn section_type(&self) -> akantu_parser::ParserType { akantu_parser::ParserType::Material }
//! }
//! let mut m = Elastic::default();
//! m.set_param("E", ParamValue::Real(2e11)).unwrap();
//! assert_eq!(m.get_param("E").unwrap().as_real().unwrap(), 2e11);
//!
//! let p = akantu_parser::Parser::parse_str("E0 = 1e9\nmaterial elastic [\n E = 2*E0\n nu = 0.3\n]", "m.dat").unwrap();
//! m.parse_section(&p.root.sub_sections[0], false).unwrap();
//! assert_eq!((m.e, m.nu), (2e9, 0.3));
//! ```
//!
//! ## Strings
//!
//! Upstream `std::string`/`ID` values are arbitrary bytes (input files need
//! not be UTF-8). Register upstream `std::string`/`ID` members as
//! [`ParamKind::ByteString`] and `std::set<std::string>`/`std::set<ID>`
//! members as [`ParamKind::ByteStringSet`]: their conversions never fail and
//! keep every byte, exactly like upstream `setAuto` (values
//! [`ParamValue::ByteString`] / [`ParamValue::ByteStringSet`]; setters use
//! [`ParamValue::into_byte_string`] / [`ParamValue::into_byte_string_set`]).
//! The Rust text kinds [`ParamKind::String`], [`ParamKind::StringList`],
//! [`ParamKind::StringSet`] and [`ParamKind::Enum`] decode UTF-8 and fail
//! on other bytes; they are a convenience for parameters that are text by
//! design (enumeration spellings, Rust/Python-side text), not the
//! upstream-compatible route. [`table_set`]/[`table_set_from_input`]
//! accept text for byte kinds (lossless) and bytes for text kinds when they
//! are valid UTF-8.
//!
//! ```
//! use akantu_parser::{ByteString, param::*};
//! #[derive(Debug, Default)]
//! struct Named { name: ByteString }
//! static NAMED_PARAMS: &[ParamDescriptor<Named>] = &[ParamDescriptor::new(
//!     "name", ParamKind::ByteString, ParameterAccess::PARSMOD, "std::string",
//!     |s| ParamValue::ByteString(s.name.clone()),
//!     Some(|s, v| { s.name = v.into_byte_string()?; Ok(()) }))];
//! # impl ParameterRegistry for Named {
//! #     fn get_param(&self, n: &str) -> akantu_core::Result<ParamValue> { table_get(NAMED_PARAMS, self, n) }
//! #     fn set_param(&mut self, n: &str, v: ParamValue) -> akantu_core::Result<()> { table_set(NAMED_PARAMS, self, n, v) }
//! #     fn param_names(&self) -> Vec<&'static str> { table_names(NAMED_PARAMS) }
//! #     fn param_info(&self, n: &str) -> Option<ParamInfo> { table_info(NAMED_PARAMS, n) }
//! #     fn set_param_from_input(&mut self, n: &str, v: ParamValue) -> akantu_core::Result<()> {
//! #         table_set_from_input(NAMED_PARAMS, self, n, v)
//! #     }
//! # }
//! # impl Parsable for Named {
//! #     fn section_type(&self) -> akantu_parser::ParserType { akantu_parser::ParserType::Global }
//! # }
//! let p = akantu_parser::Parser::parse_bytes(
//!     b"name = st\xe9el \xff \n", "n.dat", akantu_parser::AnonymousIds::Local(&mut 12)).unwrap();
//! let mut n = Named::default();
//! n.parse_section(&p.root, false).unwrap();
//! assert_eq!(n.name.as_bytes(), b"st\xe9el \xff "); // raw value, trailing space kept
//! n.set_param("name", ParamValue::String("steel".into())).unwrap(); // text is accepted
//! assert_eq!(n.name, "steel");
//! ```
//!
//! ## Write access (UD-497)
//!
//! Upstream's `ParameterTyped<T>::set` assigns without checking the access
//! flags, although `_pat_writable` is documented as "user can write into
//! the parameter" and the mutable accessor requires it. The port follows
//! the documented contract (D-003 category B):
//! [`table_set`] and the runtime parameters refuse a parameter without
//! [`ParameterAccess::WRITABLE`], and a refused
//! [`ParameterRegistry::set_param_recursive`] writes and updates nothing.
//! The compat switch [`with_upstream_write_access`] reproduces upstream on
//! the current thread for the duration of a closure: no access check (a
//! parameter needs a setter in the port), no pre-check of the recursion.
//!
//! ```
//! use akantu_parser::param::*;
//! #[derive(Debug, Default)]
//! struct Derived { lambda: f64 }
//! static DERIVED_PARAMS: &[ParamDescriptor<Derived>] = &[ParamDescriptor::new(
//!     "lambda", ParamKind::Real, ParameterAccess::READABLE, "derived",
//!     |s| ParamValue::Real(s.lambda), Some(|s, v| { s.lambda = v.as_real()?; Ok(()) }))];
//! # impl ParameterRegistry for Derived {
//! #     fn get_param(&self, n: &str) -> akantu_core::Result<ParamValue> { table_get(DERIVED_PARAMS, self, n) }
//! #     fn set_param(&mut self, n: &str, v: ParamValue) -> akantu_core::Result<()> { table_set(DERIVED_PARAMS, self, n, v) }
//! #     fn param_names(&self) -> Vec<&'static str> { table_names(DERIVED_PARAMS) }
//! #     fn param_info(&self, n: &str) -> Option<ParamInfo> { table_info(DERIVED_PARAMS, n) }
//! # }
//! let mut d = Derived::default();
//! assert!(d.set_param("lambda", ParamValue::Real(2.0)).is_err()); // default: refused
//! with_upstream_write_access(|| d.set_param("lambda", ParamValue::Real(2.0))).unwrap();
//! assert_eq!(d.lambda, 2.0); // upstream: assigned
//! ```
//!
//! ## Printing
//!
//! [`print_registry_bytes`] writes upstream `ParameterRegistry::printself`
//! byte for byte (flags, name and description widths, name order,
//! sub-registries) with each value as upstream's `ParameterTyped<T>`
//! prints it ([`ParamValue::upstream_print_bytes`]). Oracle-checked (print
//! probe, x86_64): `bool`, `std::string` with any bytes, string sets,
//! `Vector<Real>`, `Matrix<Real>`, `Int`, `UInt`, `Real`, and random
//! parameters, which print as upstream's only registrable random type,
//! `RandomInternalField [ … ]`. Not covered: plain `InternalField`
//! parameters (upstream `InternalField [ <law id>:<name> {<n> types - <m>
//! ghost types} ]`, e.g. `delta_T`: the field's identity and block layout
//! are not in a registry value) and the arrays upstream appends to both
//! internal-field descriptors at `dblDump` (row
//! PARSE-008).

use std::cell::Cell;
use std::collections::BTreeSet;
use std::fmt;
use std::ops::BitOr;

use akantu_core::random::RandomDistributionType as CoreDistributionType;
use akantu_core::tensor::MatrixX;
use akantu_core::{Error, Result};

use crate::bytes::ByteString;
use crate::expr::{self, ConversionError, DistributionKind, ParsedRandom};
use crate::scope::ScopeRef;
use crate::section::{ParserParameter, ParserSection, ParserType};
use crate::stream;

/// Access flags (upstream `ParameterAccessType`, same bit values).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParameterAccess(pub u16);

impl ParameterAccess {
    /// `_pat_internal`
    pub const INTERNAL: Self = Self(0x0001);
    /// `_pat_writable`
    pub const WRITABLE: Self = Self(0x0010);
    /// `_pat_readable`
    pub const READABLE: Self = Self(0x0100);
    /// `_pat_modifiable` = readable | writable
    pub const MODIFIABLE: Self = Self(0x0110);
    /// `_pat_parsable`
    pub const PARSABLE: Self = Self(0x1000);
    /// `_pat_parsmod` = parsable | modifiable
    pub const PARSMOD: Self = Self(0x1110);

    /// `true` if all bits of `other` are set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Upstream `Parameter::printself` access column: `iii` for internal
    /// parameters, else `r`/`w`/`p` or `-`.
    #[must_use]
    pub fn flags_string(self) -> String {
        if self.contains(Self::INTERNAL) {
            return "iii".into();
        }
        let f = |b: Self, c: char| if self.contains(b) { c } else { '-' };
        [
            f(Self::READABLE, 'r'),
            f(Self::WRITABLE, 'w'),
            f(Self::PARSABLE, 'p'),
        ]
        .into_iter()
        .collect()
    }
}

impl BitOr for ParameterAccess {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl fmt::Debug for ParameterAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ParameterAccess({:#06x})", self.0)
    }
}

/// Random distributions supported by upstream `RandomParameter`. The
/// parameters are stored **in the order written in the input file**
/// (`dist [a, b]`), i.e. the arguments of upstream
/// `make_random_parameter(base, type, a, b)`. The sampler is
/// `akantu_core::random`; [`RandomParameter::to_core`] builds it
/// through [`akantu_core::random::RandomParameter::from_parsed`], which
/// constructs the distribution exactly as upstream does, in particular
/// `weibull [a, b]` → `std::weibull_distribution(b, a)` (shape `b`, scale
/// `a`). One-parameter distributions ignore a second value; a missing
/// second value is `0`; three or more values make upstream ignore all of them
/// (`a = b = 0`).
#[derive(Clone, Debug, PartialEq)]
pub enum RandomDistribution {
    /// `uniform [a, b]` → `std::uniform_real_distribution(a, b)`
    Uniform(f64, f64),
    /// `exponential [lambda]`
    Exponential(f64),
    /// `gamma [alpha, beta]`
    Gamma(f64, f64),
    /// `weibull [a, b]` → `std::weibull_distribution(b, a)`
    Weibull(f64, f64),
    /// `extreme_value [a, b]`
    ExtremeValue(f64, f64),
    /// `normal [mean, stddev]`
    Normal(f64, f64),
    /// `lognormal [m, s]`
    Lognormal(f64, f64),
    /// `chi_squared [n]`
    ChiSquared(f64),
    /// `cauchy [a, b]`
    Cauchy(f64, f64),
    /// `fisher_f [m, n]`
    FisherF(f64, f64),
    /// `student_t [n]`
    StudentT(f64),
}

impl RandomDistribution {
    /// Upstream `RandomDistributionType` of this distribution.
    #[must_use]
    pub const fn distribution_type(&self) -> CoreDistributionType {
        match self {
            Self::Uniform(..) => CoreDistributionType::Uniform,
            Self::Exponential(..) => CoreDistributionType::Exponential,
            Self::Gamma(..) => CoreDistributionType::Gamma,
            Self::Weibull(..) => CoreDistributionType::Weibull,
            Self::ExtremeValue(..) => CoreDistributionType::ExtremeValue,
            Self::Normal(..) => CoreDistributionType::Normal,
            Self::Lognormal(..) => CoreDistributionType::Lognormal,
            Self::ChiSquared(..) => CoreDistributionType::ChiSquared,
            Self::Cauchy(..) => CoreDistributionType::Cauchy,
            Self::FisherF(..) => CoreDistributionType::FisherF,
            Self::StudentT(..) => CoreDistributionType::StudentT,
        }
    }

    /// Upstream `RandomDistributionType` spelling.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        self.distribution_type().name()
    }

    /// `(type, parameters)` as passed to upstream `make_random_parameter`
    /// (file order; one value for the one-parameter distributions).
    fn make_arguments(&self) -> (CoreDistributionType, [f64; 2], usize) {
        let ty = self.distribution_type();
        match *self {
            Self::Uniform(a, b)
            | Self::Gamma(a, b)
            | Self::Weibull(a, b)
            | Self::ExtremeValue(a, b)
            | Self::Normal(a, b)
            | Self::Lognormal(a, b)
            | Self::Cauchy(a, b)
            | Self::FisherF(a, b) => (ty, [a, b], 2),
            Self::Exponential(a) | Self::ChiSquared(a) | Self::StudentT(a) => (ty, [a, 0.0], 1),
        }
    }

    /// The freshly constructed sampler distribution (upstream
    /// `make_random_parameter(0, type, a, b).getDistribution()`).
    #[must_use]
    pub fn to_core(&self) -> akantu_core::random::Distribution {
        let (ty, args, n) = self.make_arguments();
        akantu_core::random::RandomParameter::from_parsed(0.0, ty, &args[..n])
            .distribution()
            .clone()
    }

    /// The text the oracle's `operator<<` writes for the freshly constructed
    /// standard distribution that upstream `make_random_parameter` builds
    /// from these parameters: delegates to the clean-room printing of
    /// `akantu_core::random` ([`akantu_core::random::Distribution::params_string`],
    /// D-030), checked against the parser's black-box records
    /// (`tests/fixtures/parser/stream_blackbox.jsonl`) and the random
    /// number fixtures. Every real is printed as `%.17e`; distributions that draw
    /// through an internal normal distribution append its state `0 1 0`
    /// (mean, standard deviation, no cached value) and those built on a
    /// gamma distribution append the gamma's parameters and state:
    ///
    /// | upstream distribution | text |
    /// |---|---|
    /// | `uniform(a, b)`, `extreme_value(a, b)`, `cauchy(a, b)` | `a b` |
    /// | `exponential(a)` | `a` |
    /// | `weibull(b, a)` (parameters reversed by upstream) | `b a` |
    /// | `normal(a, b)` | `a b 0` |
    /// | `gamma(a, b)`, `lognormal(a, b)` | `a b 0 1 0` |
    /// | `chi_squared(a)` | `a` + gamma(`a/2`, 1) |
    /// | `fisher_f(a, b)` | `a b` + gamma(`a/2`, 1) + gamma(`b/2`, 1) |
    /// | `student_t(a)` | `a 0 1 0` + gamma(`a/2`, 2) |
    #[must_use]
    pub fn parameters_text(&self) -> String {
        self.to_core().params_string()
    }
}

/// `base + distribution` (upstream `RandomParameter<Real>`) as parsed: a
/// plain descriptor without sampler state. Sample with
/// [`RandomParameter::to_core`] (`akantu_core::random::RandomParameter`,
/// bit-compatible with the oracle's libstdc++, `docs/ARCHITECTURE.md`).
///
/// `distribution: None` is upstream's `_not_defined` (a plain number), which
/// upstream stores as `std::uniform_real_distribution(0, 0)`: sampling it
/// still draws from the engine (`base + 0`), and it prints as
/// `uniform [ 0 0 ]`.
#[derive(Clone, Debug, PartialEq)]
pub struct RandomParameter {
    /// Deterministic base value.
    pub base_value: f64,
    /// Optional random perturbation.
    pub distribution: Option<RandomDistribution>,
}

impl RandomParameter {
    /// Upstream `make_random_parameter(base, type, a, b)` from the parsed
    /// grammar attribute (`Parser::parseRandomParameter`).
    pub(crate) fn from_parsed(p: &ParsedRandom) -> Self {
        let (a, b) = match p.parameters.as_slice() {
            [a] => (*a, 0.0),
            [a, b] => (*a, *b),
            _ => (0.0, 0.0), // 0 or >= 3 parameters: defaults
        };
        let distribution = p.kind.map(|k| match k {
            DistributionKind::Uniform => RandomDistribution::Uniform(a, b),
            DistributionKind::Exponential => RandomDistribution::Exponential(a),
            DistributionKind::Gamma => RandomDistribution::Gamma(a, b),
            DistributionKind::Weibull => RandomDistribution::Weibull(a, b),
            DistributionKind::ExtremeValue => RandomDistribution::ExtremeValue(a, b),
            DistributionKind::Normal => RandomDistribution::Normal(a, b),
            DistributionKind::Lognormal => RandomDistribution::Lognormal(a, b),
            DistributionKind::ChiSquared => RandomDistribution::ChiSquared(a),
            DistributionKind::Cauchy => RandomDistribution::Cauchy(a, b),
            DistributionKind::FisherF => RandomDistribution::FisherF(a, b),
            DistributionKind::StudentT => RandomDistribution::StudentT(a),
        });
        Self {
            base_value: p.base,
            distribution,
        }
    }

    /// The RNG ↔ parser bridge (`docs/ARCHITECTURE.md`): the
    /// sampler of `akantu_core::random` built by
    /// [`akantu_core::random::RandomParameter::from_parsed`]`(base, type,
    /// params)` — upstream `make_random_parameter` as called by
    /// `Parser::parseRandomParameter`. A plain number (`distribution: None`)
    /// becomes `_not_defined`, i.e. `uniform [0, 0]` (two engine calls per
    /// draw, as upstream). Core does not depend on the parser: this function
    /// is the only crossing.
    #[must_use]
    pub fn to_core(&self) -> akantu_core::random::RandomParameter {
        match &self.distribution {
            None => akantu_core::random::RandomParameter::from_parsed(
                self.base_value,
                CoreDistributionType::NotDefined,
                &[],
            ),
            Some(d) => {
                let (ty, args, n) = d.make_arguments();
                akantu_core::random::RandomParameter::from_parsed(self.base_value, ty, &args[..n])
            }
        }
    }
}

impl From<&RandomParameter> for akantu_core::random::RandomParameter {
    fn from(p: &RandomParameter) -> Self {
        p.to_core()
    }
}

impl From<RandomParameter> for akantu_core::random::RandomParameter {
    fn from(p: RandomParameter) -> Self {
        p.to_core()
    }
}

impl fmt::Display for RandomParameter {
    /// Upstream `RandomParameter::printself`: `"<base> + <type> [ <params> ]"`.
    /// Delegates to the `Display` of [`RandomParameter::to_core`] (the
    /// clean-room printing of `akantu_core::random`: base value in the
    /// default stream format, parameters as in
    /// [`RandomDistribution::parameters_text`]); a plain number prints as
    /// `uniform [ 0 0 ]` (upstream stores `uniform_real_distribution(0, 0)`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.to_core(), f)
    }
}

/// Type of a parameter (selects how text from the input file is parsed,
/// like the `T` of upstream `ParameterTyped<T>::setAuto`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParamKind {
    /// `Real`: algebraic expression (scalar X3 grammar, variables allowed).
    Real,
    /// `Int` (32-bit): `std::istream >> int` (never fails: `"abc"` → 0,
    /// `"3.5"` → 3, overflow → `INT_MAX`/`INT_MIN`).
    Int,
    /// `UInt` (32-bit unsigned): `std::istream >> unsigned` (`"-1"` →
    /// `4294967295`). Stored as [`ParamValue::Int`].
    UInt,
    /// `bool`: see [`crate::stream::parse_bool`].
    Bool,
    /// Rust text form of a `std::string` value: the raw value text (as
    /// [`ParamKind::ByteString`], trailing white space kept) decoded as
    /// UTF-8; a conversion error ([`ConversionError::is_invalid_utf8`]) if it
    /// is not valid UTF-8. A convenience for parameters that are text by
    /// design; upstream `std::string`/`ID` members are registered as
    /// [`ParamKind::ByteString`].
    String,
    /// `Vector<Real>` (`[a, b, c]`, Qi grammar).
    VectorReal,
    /// `Matrix<Real>` (`[[a, b], [c, d]]` or rows given by vector
    /// parameter names).
    MatrixReal,
    /// `RandomParameter<Real>`.
    Random,
    /// An enumeration: the first word of the value (upstream enum
    /// `operator>>`); convert with the enum's `FromStr`. Must be UTF-8
    /// (upstream enumeration spellings are ASCII, so a word that is not
    /// UTF-8 is not a valid spelling for upstream either).
    Enum,
    /// `std::vector<std::string>` conversion (white space and brackets
    /// removed, split on commas) as Rust text: items must be UTF-8.
    /// Upstream cannot register `std::vector<std::string>` members
    /// (`ParameterTyped<std::vector<T>>` does not compile for strings), so
    /// this kind has no upstream registry counterpart; the byte-preserving
    /// parser conversion is `Vec<ByteString>` ([`crate::FromParameter`]).
    StringList,
    /// `std::set<std::string>` as Rust text: items must be UTF-8.
    /// Upstream string-set members are registered as
    /// [`ParamKind::ByteStringSet`].
    StringSet,
    /// `std::string` / `ID` with upstream semantics — the kind for upstream
    /// `std::string`/`ID` members: the raw value bytes, unchanged (upstream
    /// `ParameterTyped<std::string>::setAuto` assigns
    /// `ParserParameter::getValue()`: every byte, including trailing white
    /// space and the white space before a trailing comment). Never fails.
    /// Value: [`ParamValue::ByteString`].
    ByteString,
    /// `std::set<std::string>` / `std::set<ID>` with upstream semantics
    /// (`ParameterTyped<std::set<T>>::setAuto` through
    /// `operator std::set<std::string>()`: white space and brackets removed,
    /// maximal runs of non-commas, byte-wise order) — the kind for upstream
    /// string-set members (e.g. `cohesive_surfaces`). Never fails. Value:
    /// [`ParamValue::ByteStringSet`].
    ByteStringSet,
}

/// A typed parameter value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ParamValue {
    /// Real.
    Real(f64),
    /// Integer (also unsigned parameters).
    Int(i64),
    /// Boolean.
    Bool(bool),
    /// Rust text ([`ParamKind::String`]).
    String(String),
    /// Real vector.
    VectorReal(Vec<f64>),
    /// Real matrix (column-major).
    MatrixReal(MatrixX),
    /// Random parameter.
    Random(RandomParameter),
    /// Enumeration spelling.
    Enum(String),
    /// List of strings (Rust text).
    StringList(Vec<String>),
    /// Ordered set of strings (Rust text).
    StringSet(BTreeSet<String>),
    /// Upstream `std::string` / `ID`: exact bytes ([`ParamKind::ByteString`]).
    ByteString(ByteString),
    /// Upstream `std::set<std::string>` / `std::set<ID>`: exact bytes,
    /// byte-wise order ([`ParamKind::ByteStringSet`]).
    ByteStringSet(BTreeSet<ByteString>),
}

impl ParamValue {
    /// What upstream `ParameterTyped<T>::printself` writes after the
    /// parameter header, line end included, as exact bytes:
    /// `stream << value << "\n"` for scalars and strings (`std::string`
    /// bytes unchanged; default stream state: `%g` with precision 6),
    /// `bool` with `std::boolalpha` (`true` / `false`,
    /// `ParameterTyped<bool>::printself`), `"[ v ... ]\n"` for vectors,
    /// sets and lists, `"[ [ a b ] [ c d ] ]\n"` for matrices (row by row).
    ///
    /// A random parameter prints as upstream's `RandomInternalField`
    /// descriptor, `"RandomInternalField [ <base> + <type> [ <params> ] ]\n"`
    /// (`random_internal_field_tmpl.hh:58-69`): upstream can register a
    /// random parameter only as a `RandomInternalField` member (a bare
    /// `RandomParameter` does not compile in `ParameterTyped<T>::to_string`,
    /// oracle print probe). Not reproduced (PARSE-008
    /// scope): the field arrays that upstream appends to both internal-field
    /// descriptors at `dblDump` and above, and the plain `InternalField`
    /// descriptor `"InternalField [ <law id>:<name> {<n> types - <m> ghost
    /// types} ]"` (e.g. `delta_T`), whose field identity and block layout a
    /// registry value does not carry.
    #[must_use]
    pub fn upstream_print_bytes(&self) -> Vec<u8> {
        fn list<'a>(items: impl IntoIterator<Item = std::borrow::Cow<'a, [u8]>>) -> Vec<u8> {
            let mut out = b"[ ".to_vec();
            for i in items {
                out.extend_from_slice(&i);
                out.push(b' ');
            }
            out.extend_from_slice(b"]\n");
            out
        }
        let line = |text: &[u8]| {
            let mut out = text.to_vec();
            out.push(b'\n');
            out
        };
        let real = |x: f64| std::borrow::Cow::Owned(stream::format_double(x).into_bytes());
        match self {
            Self::Real(v) => line(stream::format_double(*v).as_bytes()),
            Self::Int(v) => line(v.to_string().as_bytes()),
            Self::Bool(b) => line(if *b { b"true" } else { b"false" }),
            Self::String(t) | Self::Enum(t) => line(t.as_bytes()),
            Self::ByteString(b) => line(b.as_bytes()),
            Self::Random(r) => line(format!("RandomInternalField [ {r} ]").as_bytes()),
            Self::VectorReal(v) => list(v.iter().map(|&x| real(x))),
            Self::StringList(v) => list(v.iter().map(|t| t.as_bytes().into())),
            Self::StringSet(v) => list(v.iter().map(|t| t.as_bytes().into())),
            Self::ByteStringSet(v) => list(v.iter().map(|b| b.as_bytes().into())),
            Self::MatrixReal(m) => {
                let mut out = b"[ ".to_vec();
                for i in 0..m.rows() {
                    if m.cols() != 1 {
                        out.extend_from_slice(b"[ ");
                    }
                    for j in 0..m.cols() {
                        out.extend_from_slice(stream::format_double(m.at(i, j)).as_bytes());
                        out.push(b' ');
                    }
                    if m.cols() != 1 {
                        out.extend_from_slice(b"] ");
                    }
                }
                out.extend_from_slice(b"]\n");
                out
            }
        }
    }

    /// [`ParamValue::upstream_print_bytes`] as text: bytes that are not
    /// UTF-8 (`std::string` parameters) print lossily.
    #[must_use]
    pub fn upstream_print(&self) -> String {
        String::from_utf8_lossy(&self.upstream_print_bytes()).into_owned()
    }

    /// Kind of this value (`Int` for both signed and unsigned integers).
    #[must_use]
    pub fn kind(&self) -> ParamKind {
        match self {
            Self::Real(_) => ParamKind::Real,
            Self::Int(_) => ParamKind::Int,
            Self::Bool(_) => ParamKind::Bool,
            Self::String(_) => ParamKind::String,
            Self::VectorReal(_) => ParamKind::VectorReal,
            Self::MatrixReal(_) => ParamKind::MatrixReal,
            Self::Random(_) => ParamKind::Random,
            Self::Enum(_) => ParamKind::Enum,
            Self::StringList(_) => ParamKind::StringList,
            Self::StringSet(_) => ParamKind::StringSet,
            Self::ByteString(_) => ParamKind::ByteString,
            Self::ByteStringSet(_) => ParamKind::ByteStringSet,
        }
    }

    /// Real value; integers convert (upstream `operator Real()`).
    pub fn as_real(&self) -> Result<f64> {
        match self {
            Self::Real(v) => Ok(*v),
            #[allow(clippy::cast_precision_loss)] // upstream converts Int → Real implicitly
            Self::Int(v) => Ok(*v as f64),
            Self::Random(r) if r.distribution.is_none() => Ok(r.base_value),
            other => Err(type_error("Real", other)),
        }
    }

    /// Integer value.
    pub fn as_int(&self) -> Result<i64> {
        match self {
            Self::Int(v) => Ok(*v),
            other => Err(type_error("Int", other)),
        }
    }

    /// Boolean value.
    pub fn as_bool(&self) -> Result<bool> {
        match self {
            Self::Bool(v) => Ok(*v),
            other => Err(type_error("bool", other)),
        }
    }

    /// String value as Rust text (strings, enum spellings, and byte strings
    /// that are valid UTF-8; an error for other bytes — the explicit text
    /// boundary, see [`ParamValue::as_bytes`]).
    pub fn as_str(&self) -> Result<&str> {
        match self {
            Self::String(s) | Self::Enum(s) => Ok(s),
            Self::ByteString(b) => b.to_str().ok_or_else(|| not_utf8("string", b)),
            other => Err(type_error("string", other)),
        }
    }

    /// String value as bytes (byte strings, strings, enum spellings).
    pub fn as_bytes(&self) -> Result<&[u8]> {
        match self {
            Self::ByteString(b) => Ok(b.as_bytes()),
            Self::String(s) | Self::Enum(s) => Ok(s.as_bytes()),
            other => Err(type_error("std::string", other)),
        }
    }

    /// String value as a [`ByteString`] (byte strings, strings, enum
    /// spellings): the setter-side conversion of a [`ParamKind::ByteString`]
    /// parameter.
    pub fn into_byte_string(self) -> Result<ByteString> {
        match self {
            Self::ByteString(b) => Ok(b),
            Self::String(s) | Self::Enum(s) => Ok(s.into()),
            other => Err(type_error("std::string", &other)),
        }
    }

    /// String-set value as byte strings (byte-string sets and text sets):
    /// the setter-side conversion of a [`ParamKind::ByteStringSet`]
    /// parameter.
    pub fn into_byte_string_set(self) -> Result<BTreeSet<ByteString>> {
        match self {
            Self::ByteStringSet(s) => Ok(s),
            Self::StringSet(s) => Ok(s.into_iter().map(ByteString::from).collect()),
            other => Err(type_error("std::set<std::string>", &other)),
        }
    }

    /// Parse the raw text of an input-file parameter as `kind`
    /// (upstream `ParameterTyped<T>::setAuto`). Algebraic expressions may
    /// refer to variables of `scope` and of its ancestors (a section
    /// obtained from a parsed [`crate::Parser`] knows its ancestors).
    ///
    /// Errors are [`Error::Parse`] with file `<string>`, line 1 and the
    /// 1-based byte column where the conversion failed.
    pub fn parse(kind: ParamKind, text: &str, scope: &ParserSection) -> Result<ParamValue> {
        Self::parse_bytes(kind, text.as_bytes(), scope)
    }

    /// [`ParamValue::parse`] for raw bytes (values that need not be UTF-8,
    /// e.g. for [`ParamKind::ByteString`]).
    pub fn parse_bytes(kind: ParamKind, text: &[u8], scope: &ParserSection) -> Result<ParamValue> {
        convert(kind, text, ScopeRef::Section(scope)).map_err(|e| Error::Parse {
            file: "<string>".into(),
            line: 1,
            column: e.offset() + 1,
            message: format!(
                "cannot convert '{}' to {kind:?}: {}",
                String::from_utf8_lossy(text),
                e.message()
            ),
        })
    }
}

/// Typed conversion of the raw bytes `text` in `scope`. The byte kinds
/// (`ByteString`, `ByteStringSet`) keep every byte and never fail; the Rust
/// text kinds (`String`, `Enum`, `StringList`, `StringSet`) require valid
/// UTF-8 (an explicit conversion error otherwise, never a lossy
/// replacement).
pub(crate) fn convert<'x>(
    kind: ParamKind,
    text: &'x [u8],
    scope: ScopeRef<'x>,
) -> std::result::Result<ParamValue, ConversionError> {
    let text_string = |b: &[u8]| crate::section::utf8(ByteString::from(b));
    let strings = |items: Vec<Vec<u8>>| -> std::result::Result<Vec<String>, ConversionError> {
        items
            .into_iter()
            .map(|b| crate::section::utf8(ByteString::from(b)))
            .collect()
    };
    Ok(match kind {
        ParamKind::Real => ParamValue::Real(expr::eval_real(text, scope)?),
        ParamKind::Int => ParamValue::Int(i64::from(stream::parse_int::<i32>(text))),
        ParamKind::UInt => ParamValue::Int(i64::from(stream::parse_int::<u32>(text))),
        ParamKind::Bool => ParamValue::Bool(stream::parse_bool(text)),
        ParamKind::String => ParamValue::String(text_string(text)?),
        ParamKind::VectorReal => ParamValue::VectorReal(expr::eval_vector(text, scope)?),
        ParamKind::MatrixReal => {
            ParamValue::MatrixReal(matrix_from_rows(&expr::eval_matrix(text, scope)?)?)
        }
        ParamKind::Random => ParamValue::Random(RandomParameter::from_parsed(&expr::eval_random(
            text, scope,
        )?)),
        ParamKind::Enum => ParamValue::Enum(text_string(stream::word_bytes(text))?),
        ParamKind::StringList => ParamValue::StringList(strings(stream::string_list_bytes(text))?),
        ParamKind::StringSet => ParamValue::StringSet(
            strings(stream::string_list_bytes(text))?
                .into_iter()
                .collect(),
        ),
        ParamKind::ByteString => ParamValue::ByteString(ByteString::from(text)),
        ParamKind::ByteStringSet => ParamValue::ByteStringSet(
            stream::string_list_bytes(text)
                .into_iter()
                .map(ByteString::from)
                .collect(),
        ),
    })
}

/// Upstream `parsable_matrix::operator Matrix<Real>()`: `rows × max(cols)`
/// zero-filled, then the given entries.
pub(crate) fn matrix_from_rows(rows: &[Vec<f64>]) -> std::result::Result<MatrixX, ConversionError> {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut m = MatrixX::zeros(rows.len(), cols);
    for (i, row) in rows.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            *m.at_mut(i, j) = v;
        }
    }
    Ok(m)
}

/// Upstream `ParameterTyped<Vector<T>>::setAuto`: when the target already
/// has a size, only the first `current_len` parsed values are used. Upstream
/// reads out of bounds if fewer values are given; here that is an error.
pub fn fit_vector(current_len: usize, parsed: Vec<f64>) -> Result<Vec<f64>> {
    if current_len == 0 {
        return Ok(parsed);
    }
    if parsed.len() < current_len {
        return Err(Error::DimensionMismatch {
            context: "vector parameter",
            expected: current_len,
            got: parsed.len(),
        });
    }
    let mut parsed = parsed;
    parsed.truncate(current_len);
    Ok(parsed)
}

/// Upstream `ParameterTyped<Matrix<T>>::setAuto`: when the target already
/// has a size, the top-left `rows × cols` block of the parsed matrix is
/// used (an error if the parsed matrix is smaller).
pub fn fit_matrix(rows: usize, cols: usize, parsed: MatrixX) -> Result<MatrixX> {
    if rows == 0 || cols == 0 {
        return Ok(parsed);
    }
    if parsed.rows() < rows || parsed.cols() < cols {
        return Err(Error::DimensionMismatch {
            context: "matrix parameter",
            expected: rows * cols,
            got: parsed.rows() * parsed.cols(),
        });
    }
    let mut m = MatrixX::zeros(rows, cols);
    for j in 0..cols {
        for i in 0..rows {
            *m.at_mut(i, j) = parsed.at(i, j);
        }
    }
    Ok(m)
}

fn type_error(expected: &str, got: &ParamValue) -> Error {
    Error::invalid(format!(
        "parameter type mismatch: expected {expected}, got {:?}",
        got.kind()
    ))
}

fn not_utf8(expected: &str, got: &ByteString) -> Error {
    Error::invalid(format!(
        "parameter value {got:?} is not valid UTF-8 and cannot be converted to {expected} (use the ByteString kinds to keep the bytes)"
    ))
}

/// Getter of a registered parameter.
pub type ParamGetter<S> = fn(&S) -> ParamValue;
/// Setter of a registered parameter.
pub type ParamSetter<S> = fn(&mut S, ParamValue) -> Result<()>;

/// Static description of one parameter of type `S`.
pub struct ParamDescriptor<S> {
    /// Name (as in input files and Python).
    pub name: &'static str,
    /// Type.
    pub kind: ParamKind,
    /// Access flags.
    pub access: ParameterAccess,
    /// Human-readable description (upstream registerParam description).
    pub description: &'static str,
    /// Getter.
    pub get: ParamGetter<S>,
    /// Setter (`None` for read-only parameters).
    pub set: Option<ParamSetter<S>>,
}

impl<S> ParamDescriptor<S> {
    /// `const` constructor for static tables.
    pub const fn new(
        name: &'static str,
        kind: ParamKind,
        access: ParameterAccess,
        description: &'static str,
        get: ParamGetter<S>,
        set: Option<ParamSetter<S>>,
    ) -> Self {
        Self {
            name,
            kind,
            access,
            description,
            get,
            set,
        }
    }
}

impl<S> fmt::Debug for ParamDescriptor<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParamDescriptor")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("access", &self.access)
            .finish_non_exhaustive()
    }
}

/// Kind and access flags of a registered parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParamInfo {
    /// Type.
    pub kind: ParamKind,
    /// Access flags.
    pub access: ParameterAccess,
}

thread_local! {
    /// Whether [`with_upstream_write_access`] is active on this thread.
    static UPSTREAM_WRITE_ACCESS: Cell<bool> = const { Cell::new(false) };
}

/// UD-497 compat switch (D-003 category B): run `f` with upstream's write
/// semantics for parameter registries on the current thread — `set` of a
/// parameter without [`ParameterAccess::WRITABLE`] assigns it (upstream
/// `ParameterTyped<T>::set` has no access check) and
/// [`ParameterRegistry::set_param_recursive`] does not pre-check the
/// recursion, so a failure leaves the registries reached before it updated,
/// as an upstream exception does. Applies to [`table_set`] and the runtime
/// parameters of `akantu-model` (`DynamicParameters`); a parameter without
/// a setter in the port (a derived quantity without storage) is still an
/// error. The previous state is restored when `f` returns or unwinds;
/// scopes nest; other threads are not affected.
pub fn with_upstream_write_access<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            UPSTREAM_WRITE_ACCESS.with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(UPSTREAM_WRITE_ACCESS.with(|c| c.replace(true)));
    f()
}

/// `true` inside [`with_upstream_write_access`] on the current thread.
#[must_use]
pub fn upstream_write_access() -> bool {
    UPSTREAM_WRITE_ACCESS.with(Cell::get)
}

/// The refusal of a write to a parameter without `WRITABLE` (UD-497).
#[must_use]
pub fn not_writable(name: &str) -> Error {
    Error::invalid(format!("Parameter {name} is not writable"))
}

/// Upstream `ParameterRegistry`: named, typed, access-controlled parameters.
/// Object-safe so that Python and the dumpers can use `dyn ParameterRegistry`.
pub trait ParameterRegistry {
    /// Read a parameter (requires `READABLE`).
    fn get_param(&self, name: &str) -> Result<ParamValue>;
    /// Write a parameter (requires `WRITABLE`), then
    /// [`ParameterRegistry::update_internal_parameters`] must be called by the
    /// caller or the implementation.
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()>;
    /// Registered names, in registration order.
    fn param_names(&self) -> Vec<&'static str>;
    /// Recompute derived quantities after parameters changed (upstream
    /// `updateInternalParameters`, e.g. Lamé constants from E and ν).
    fn update_internal_parameters(&mut self) -> Result<()> {
        Ok(())
    }
    /// Kind and access flags of a registered parameter (`None` if the name
    /// is not registered). Required for [`Parsable::parse_section`]; use
    /// [`table_info`].
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        let _ = name;
        None
    }
    /// Set a parameter from an input file (upstream `Parameter::setAuto`):
    /// requires `PARSABLE` (not `WRITABLE`). Use [`table_set_from_input`].
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        let _ = value;
        Err(Error::not_found("parameter", name))
    }

    /// Upstream `hasParameter(name)`: registered in **this** registry (the
    /// sub-registries are not searched, as upstream).
    fn has_parameter(&self, name: &str) -> bool {
        self.param_names().contains(&name)
    }

    /// Description given at registration (upstream `registerParam(...,
    /// description)`), printed by [`print_registry`]. Use
    /// [`table_description`].
    fn param_description(&self, name: &str) -> Option<String> {
        let _ = name;
        None
    }

    /// Value of a registered parameter for printing: upstream `printself`
    /// prints every parameter whatever its access rights. The default reads
    /// it with [`ParameterRegistry::get_param`] (readable parameters only);
    /// [`table_print_value`] reads any table entry.
    fn param_print_value(&self, name: &str) -> Option<ParamValue> {
        self.get_param(name).ok()
    }

    /// Ids of the registered sub-registries (upstream
    /// `registerSubRegistry(id, registry)`, and `Parsable::registerSubSection`
    /// which registers the sub-parsable under its name), in any order;
    /// duplicates resolve to the last registration (upstream
    /// `insert_or_assign`). Use [`table_sub_registry_ids`].
    fn sub_registry_ids(&self) -> Vec<&str> {
        Vec::new()
    }

    /// The sub-registry registered under `id`. Use [`table_sub_registry`].
    fn sub_registry(&self, id: &str) -> Option<&dyn ParameterRegistry> {
        let _ = id;
        None
    }

    /// Mutable sub-registry registered under `id`. Use
    /// [`table_sub_registry_mut`].
    fn sub_registry_mut(&mut self, id: &str) -> Option<&mut dyn ParameterRegistry> {
        let _ = id;
        None
    }

    /// Upstream `consisder_sub`: whether [`ParameterRegistry::get_param_recursive`]
    /// and [`ParameterRegistry::set_param_recursive`] search the
    /// sub-registries. Upstream: `true` for a plain `ParameterRegistry`
    /// (e.g. `TimeStepSolver` with its `non_linear_solver`), `false` for
    /// every `Parsable` (its sub-sections are parsed, not searched by
    /// `get`/`set`). The port's default is `false` — every registry of the
    /// port is a [`Parsable`] or a leaf — so a plain registry that registers
    /// sub-registries must return `true`.
    fn considers_sub_registries(&self) -> bool {
        false
    }

    /// Upstream `listSubRegisteries()`: the sub-registry ids in `std::map`
    /// order (byte-wise sorted, unique).
    fn list_sub_registries(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .sub_registry_ids()
            .into_iter()
            .map(str::to_owned)
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Upstream `ParameterRegistry::get(name)`: the parameter of this
    /// registry, else — when [`ParameterRegistry::considers_sub_registries`]
    /// — of the first sub-registry (in [`ParameterRegistry::list_sub_registries`]
    /// order) that has it, searched recursively with each sub-registry's
    /// own flag. Readability is then checked by
    /// [`ParameterRegistry::get_param`] (upstream checks it when converting
    /// the found parameter).
    ///
    /// # Errors
    /// [`Error::NotFound`] if no registry of the search has `name`; the
    /// owner's access error otherwise.
    fn get_param_recursive(&self, name: &str) -> Result<ParamValue> {
        if self.has_parameter(name) {
            return self.get_param(name);
        }
        if self.considers_sub_registries() {
            for id in self.list_sub_registries() {
                if let Some(sub) = self.sub_registry(&id)
                    && registry_finds(sub, name)
                {
                    return sub.get_param_recursive(name);
                }
            }
        }
        Err(not_found_in_scope(name, &self.param_names()))
    }

    /// Upstream `ParameterRegistry::set(name, value)`: sets the parameter of
    /// this registry; else — when
    /// [`ParameterRegistry::considers_sub_registries`] — calls this method
    /// on **every** sub-registry in id order (so a name present in several
    /// sub-registries is set in all of them, and a name present in none is
    /// silently ignored, as upstream); without sub-registry search an
    /// unknown name is an error. Each registry reached calls
    /// [`ParameterRegistry::update_internal_parameters`] afterwards
    /// (sub-registries before their parent, as upstream).
    ///
    /// Write access (UD-497, D-003 category B): the port's setters refuse a
    /// parameter without `WRITABLE` ([`table_set`]); upstream
    /// `ParameterTyped::set` never checks the access type. The access of
    /// every registry the recursion would write is checked first
    /// ([`ParameterRegistry::check_set_access_recursive`]), so a refusal
    /// writes and updates nothing. Other failures (value kind, setter and
    /// update errors, an unknown name in a registry that does not search
    /// its sub-registries) stop the recursion where they occur, like an
    /// upstream exception: registries reached before keep their new values.
    /// Inside [`with_upstream_write_access`] there is no access check and
    /// no pre-check (upstream).
    ///
    /// # Errors
    /// [`Error::NotFound`] for an unknown name when sub-registries are not
    /// searched; the refusal of a parameter without `WRITABLE`; setter and
    /// update errors.
    fn set_param_recursive(&mut self, name: &str, value: ParamValue) -> Result<()> {
        if !upstream_write_access() {
            self.check_set_access_recursive(name)?;
        }
        self.set_param_recursive_unchecked(name, value)
    }

    /// The access pre-check of [`ParameterRegistry::set_param_recursive`]:
    /// walks the same registries (this one if it has `name`, else — when it
    /// searches them — every sub-registry, recursively) and refuses the
    /// first parameter `name` whose [`ParameterRegistry::param_info`] lacks
    /// `WRITABLE`. Registries without the name, or without `param_info`,
    /// pass (their own `set` decides). Nothing is modified.
    ///
    /// # Errors
    /// [`not_writable`] for the first refused parameter.
    fn check_set_access_recursive(&self, name: &str) -> Result<()> {
        if self.has_parameter(name) {
            return match self.param_info(name) {
                Some(info) if !info.access.contains(ParameterAccess::WRITABLE) => {
                    Err(not_writable(name))
                }
                _ => Ok(()),
            };
        }
        if self.considers_sub_registries() {
            for id in self.list_sub_registries() {
                if let Some(sub) = self.sub_registry(&id) {
                    sub.check_set_access_recursive(name)?;
                }
            }
        }
        Ok(())
    }

    /// Upstream `ParameterRegistry::set` without the port's access
    /// pre-check: the recursion of [`ParameterRegistry::set_param_recursive`]
    /// (each sub-registry is reached through its own
    /// `set_param_recursive_unchecked`; each leaf `set_param` still applies
    /// its own access rule unless [`with_upstream_write_access`] is active).
    ///
    /// # Errors
    /// As [`ParameterRegistry::set_param_recursive`].
    fn set_param_recursive_unchecked(&mut self, name: &str, value: ParamValue) -> Result<()> {
        if self.has_parameter(name) {
            self.set_param(name, value)?;
        } else if self.considers_sub_registries() {
            for id in self.list_sub_registries() {
                if let Some(sub) = self.sub_registry_mut(&id) {
                    sub.set_param_recursive_unchecked(name, value.clone())?;
                }
            }
        } else {
            return Err(not_found_in_scope(name, &self.param_names()));
        }
        self.update_internal_parameters()
    }
}

/// Whether upstream `get(name)` on `r` finds a parameter (this registry,
/// else its sub-registries when it searches them, recursively).
fn registry_finds(r: &dyn ParameterRegistry, name: &str) -> bool {
    r.has_parameter(name)
        || (r.considers_sub_registries()
            && r.list_sub_registries().iter().any(|id| {
                r.sub_registry(id)
                    .is_some_and(|sub| registry_finds(sub, name))
            }))
}

/// Upstream `ParameterUnexistingException` text (without the location).
fn not_found_in_scope(name: &str, names: &[&str]) -> Error {
    let mut sorted = names.to_vec();
    sorted.sort_unstable();
    Error::not_found(
        "parameter",
        format!(
            "{name} (Parameter {name} does not exists in this scope; possible parameters are: {})",
            sorted.join(", ")
        ),
    )
}

/// Upstream `Parameter::printself` followed by the value
/// (`ParameterTyped<T>::printself`): `" <flags> <name>[ [<description>]] : <value>"`
/// with upstream's stream widths, and the line end each upstream type
/// prints (exact bytes).
fn print_parameter(
    out: &mut Vec<u8>,
    name: &str,
    info: ParamInfo,
    description: Option<&str>,
    value: Option<&ParamValue>,
) {
    out.push(b' ');
    out.extend_from_slice(info.access.flags_string().as_bytes());
    out.push(b' ');
    // sstr << name; sstr.width(max(10 - len, 0)); sstr << " [" << description << "]"
    let mut s = name.to_owned();
    if let Some(d) = description.filter(|d| !d.is_empty()) {
        let width = 10usize.saturating_sub(name.len());
        s.push_str(&format!("{:>width$}", " ["));
        s.push_str(d);
        s.push(']');
    }
    out.extend_from_slice(s.as_bytes());
    // stream.width(max(50 - len, 0)); stream << " : "
    let width = 50usize.saturating_sub(s.len());
    out.extend_from_slice(format!("{:>width$}", " : ").as_bytes());
    match value {
        Some(v) => out.extend_from_slice(&v.upstream_print_bytes()),
        None => out.extend_from_slice(b"<unknown>\n"),
    }
}

/// Upstream `ParameterRegistry::printself(stream, indent)` as the exact
/// bytes upstream streams: the parameters in name order, then every
/// sub-registry (id order) as `Registry [\n<sub-registry at indent +
/// 1><indent>]` — upstream writes no line end after the closing bracket.
/// `std::string` values are written unchanged.
#[must_use]
pub fn print_registry_bytes(registry: &dyn ParameterRegistry, indent: usize) -> Vec<u8> {
    let mut out = Vec::new();
    write_registry(&mut out, registry, indent);
    out
}

fn write_registry(out: &mut Vec<u8>, registry: &dyn ParameterRegistry, indent: usize) {
    let space = " ".repeat(indent);
    let mut names = registry.param_names();
    names.sort_unstable();
    for name in names {
        let Some(info) = registry.param_info(name) else {
            continue;
        };
        out.extend_from_slice(space.as_bytes());
        let description = registry.param_description(name);
        let value = registry.param_print_value(name);
        print_parameter(out, name, info, description.as_deref(), value.as_ref());
    }
    for id in registry.list_sub_registries() {
        if let Some(sub) = registry.sub_registry(&id) {
            out.extend_from_slice(space.as_bytes());
            out.extend_from_slice(b"Registry [\n");
            write_registry(out, sub, indent + 1);
            out.extend_from_slice(space.as_bytes());
            out.push(b']');
        }
    }
}

/// [`print_registry_bytes`] as text (bytes that are not UTF-8, possible in
/// `std::string` parameters only, print lossily).
#[must_use]
pub fn print_registry(registry: &dyn ParameterRegistry, indent: usize) -> String {
    String::from_utf8_lossy(&print_registry_bytes(registry, indent)).into_owned()
}

/// Getter of a registered sub-registry (upstream
/// `registerSubRegistry(id, member)`).
pub type SubRegistryGetter<S> = for<'a> fn(&'a S) -> &'a dyn ParameterRegistry;
/// Mutable getter of a registered sub-registry.
pub type SubRegistryGetterMut<S> = for<'a> fn(&'a mut S) -> &'a mut dyn ParameterRegistry;
/// Getter of a registered sub-section parsable (upstream
/// `registerSubSection(type, name, member)`).
pub type SubParsableGetter<S> = for<'a> fn(&'a S) -> &'a dyn Parsable;
/// Mutable getter of a registered sub-section parsable.
pub type SubParsableGetterMut<S> = for<'a> fn(&'a mut S) -> &'a mut dyn Parsable;

/// Static description of a sub-registry of type `S` (upstream
/// `ParameterRegistry::registerSubRegistry(id, registry)`, D-016: an accessor
/// instead of a stored reference to a member).
pub struct SubRegistryDescriptor<S> {
    /// Registration id.
    pub id: &'static str,
    /// Accessor.
    pub get: SubRegistryGetter<S>,
    /// Mutable accessor.
    pub get_mut: SubRegistryGetterMut<S>,
}

impl<S> SubRegistryDescriptor<S> {
    /// `const` constructor for static tables.
    pub const fn new(
        id: &'static str,
        get: SubRegistryGetter<S>,
        get_mut: SubRegistryGetterMut<S>,
    ) -> Self {
        Self { id, get, get_mut }
    }
}

impl<S> fmt::Debug for SubRegistryDescriptor<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubRegistryDescriptor")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Static description of a sub-section parsable of type `S` (upstream
/// `Parsable::registerSubSection(type, name, parsable)`: dispatched by
/// [`Parsable::parse_sub_section`] for sections of that type and name, and
/// a sub-registry under `name`).
pub struct SubSectionDescriptor<S> {
    /// Section type of the sub-section.
    pub section_type: ParserType,
    /// Section name (`<type> <name> [`), also the sub-registry id.
    pub name: &'static str,
    /// Accessor.
    pub get: SubParsableGetter<S>,
    /// Mutable accessor.
    pub get_mut: SubParsableGetterMut<S>,
}

impl<S> SubSectionDescriptor<S> {
    /// `const` constructor for static tables.
    pub const fn new(
        section_type: ParserType,
        name: &'static str,
        get: SubParsableGetter<S>,
        get_mut: SubParsableGetterMut<S>,
    ) -> Self {
        Self {
            section_type,
            name,
            get,
            get_mut,
        }
    }
}

impl<S> fmt::Debug for SubSectionDescriptor<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubSectionDescriptor")
            .field("section_type", &self.section_type)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Table-driven [`ParameterRegistry::sub_registry_ids`]: the ids of the
/// plain sub-registries, then the names of the sub-sections (registration
/// order of the two tables).
pub fn table_sub_registry_ids<S>(
    registries: &[SubRegistryDescriptor<S>],
    sections: &[SubSectionDescriptor<S>],
) -> Vec<&'static str> {
    registries
        .iter()
        .map(|d| d.id)
        .chain(sections.iter().map(|d| d.name))
        .collect()
}

/// Table-driven [`ParameterRegistry::sub_registry`]: the last registration
/// of `id` (upstream `insert_or_assign`; a sub-section counts as
/// registered after the plain sub-registries).
pub fn table_sub_registry<'a, S>(
    registries: &[SubRegistryDescriptor<S>],
    sections: &[SubSectionDescriptor<S>],
    s: &'a S,
    id: &str,
) -> Option<&'a dyn ParameterRegistry> {
    if let Some(d) = sections.iter().rev().find(|d| d.name == id) {
        let p: &'a dyn Parsable = (d.get)(s);
        return Some(p);
    }
    registries
        .iter()
        .rev()
        .find(|d| d.id == id)
        .map(|d| (d.get)(s))
}

/// Table-driven [`ParameterRegistry::sub_registry_mut`].
pub fn table_sub_registry_mut<'a, S>(
    registries: &[SubRegistryDescriptor<S>],
    sections: &[SubSectionDescriptor<S>],
    s: &'a mut S,
    id: &str,
) -> Option<&'a mut dyn ParameterRegistry> {
    if let Some(d) = sections.iter().rev().find(|d| d.name == id) {
        let p: &'a mut dyn Parsable = (d.get_mut)(s);
        return Some(p);
    }
    registries
        .iter()
        .rev()
        .find(|d| d.id == id)
        .map(|d| (d.get_mut)(s))
}

/// Table-driven [`Parsable::sub_parsable_mut`]: the last registration of
/// `(section_type, name)` (upstream `insert_or_assign`).
pub fn table_sub_parsable_mut<'a, S>(
    sections: &[SubSectionDescriptor<S>],
    s: &'a mut S,
    section_type: ParserType,
    name: &str,
) -> Option<&'a mut dyn Parsable> {
    sections
        .iter()
        .rev()
        .find(|d| d.section_type == section_type && d.name == name)
        .map(|d| (d.get_mut)(s))
}

/// Table-driven [`ParameterRegistry::param_description`].
pub fn table_description<S>(table: &[ParamDescriptor<S>], name: &str) -> Option<String> {
    table
        .iter()
        .find(|d| d.name == name)
        .map(|d| d.description.to_owned())
}

/// Table-driven [`ParameterRegistry::param_print_value`]: the getter of the
/// entry, whatever its access rights (upstream `printself`).
pub fn table_print_value<S>(table: &[ParamDescriptor<S>], s: &S, name: &str) -> Option<ParamValue> {
    table.iter().find(|d| d.name == name).map(|d| (d.get)(s))
}

/// Upstream `Parsable`: fill a registry from an input-file section.
pub trait Parsable: ParameterRegistry {
    /// Section type this object reads.
    fn section_type(&self) -> ParserType;

    /// Identifier used in messages (upstream `pid`).
    fn parsable_id(&self) -> &str {
        ""
    }

    /// Upstream `Parsable::parseSection`: checks the section type, applies
    /// every parameter in upstream order (sorted by name) with
    /// [`Parsable::parse_param`], dispatches the sub-sections (upstream
    /// order) to [`Parsable::parse_sub_section`], then calls
    /// [`ParameterRegistry::update_internal_parameters`]. `permissive` is
    /// upstream `Parser::isPermissive()` (see
    /// [`crate::Parser::is_permissive`]).
    fn parse_section(&mut self, section: &ParserSection, permissive: bool) -> Result<()> {
        if section.section_type != self.section_type() {
            return Err(Error::invalid(format!(
                "The object {} is meant to parse section of type {}, so it cannot parse section of type {}",
                self.parsable_id(),
                self.section_type(),
                section.section_type
            )));
        }
        for p in section.parameters_sorted() {
            self.parse_param(section, p, permissive)?;
        }
        for s in section.sub_sections_sorted() {
            self.parse_sub_section(s, permissive)?;
        }
        self.update_internal_parameters()
    }

    /// Upstream `Parsable::parseParam`: unknown names are an error, or a
    /// warning (`log::warn!`) in permissive mode; parameters that are not
    /// `PARSABLE` are an error; the value is converted according to the
    /// registered kind, with variables resolved in `section` and its
    /// ancestors ([`ParamKind::ByteString`]/[`ParamKind::ByteStringSet`]
    /// keep every byte, as upstream; the Rust text kinds fail on values that
    /// are not UTF-8).
    fn parse_param(
        &mut self,
        section: &ParserSection,
        param: &ParserParameter,
        permissive: bool,
    ) -> Result<()> {
        let located = |message: String| Error::Parse {
            file: param.file.clone(),
            line: param.line,
            column: param.column,
            message,
        };
        // A name that is not UTF-8 cannot be registered (registry names are
        // Rust strings): it is unknown, as upstream would find it unknown.
        let name = param.name.to_str();
        let Some(info) = name.and_then(|n| self.param_info(n)) else {
            let msg = format!(
                "No parameter named {} registered in {}.",
                param.name,
                self.parsable_id()
            );
            if permissive {
                log::warn!("{msg}");
                return Ok(());
            }
            return Err(located(msg));
        };
        if !info.access.contains(ParameterAccess::PARSABLE) {
            return Err(located(format!("Parameter {} is not parsable", param.name)));
        }
        let value = convert(info.kind, &param.value, ScopeRef::Section(section))
            .map_err(|e| param.conversion_error(&format!("{:?}", info.kind), &e))?;
        self.set_param_from_input(name.unwrap_or_default(), value)
    }

    /// The sub-parsable registered for sub-sections `<section_type
    /// name [ ... ]>` (upstream `registerSubSection(type, name, parsable)`,
    /// `std::map` keyed by `(type, name)`, last registration wins). Use
    /// [`table_sub_parsable_mut`]; register the same children as
    /// sub-registries ([`table_sub_registry_ids`]), as upstream does.
    fn sub_parsable_mut(
        &mut self,
        section_type: ParserType,
        name: &str,
    ) -> Option<&mut dyn Parsable> {
        let _ = (section_type, name);
        None
    }

    /// Upstream `Parsable::parseSubSection`: the sub-parsable registered for
    /// `(section.section_type, section.name)` parses the section
    /// ([`Parsable::parse_section`]: its parameters, its own sub-sections,
    /// its update); without one, an error, or a warning in permissive mode.
    fn parse_sub_section(&mut self, section: &ParserSection, permissive: bool) -> Result<()> {
        if let Some(child) = self.sub_parsable_mut(section.section_type, &section.name) {
            return child.parse_section(section, permissive);
        }
        let msg = format!(
            "No parsable defined for sub sections of type <{},{}> in {}",
            section.section_type,
            section.name,
            self.parsable_id()
        );
        if permissive {
            log::warn!("{msg}");
            Ok(())
        } else {
            Err(Error::invalid(msg))
        }
    }
}

/// Table-driven `get_param`.
pub fn table_get<S>(table: &[ParamDescriptor<S>], s: &S, name: &str) -> Result<ParamValue> {
    let d = find(table, name)?;
    if !d.access.contains(ParameterAccess::READABLE) {
        return Err(Error::invalid(format!("Parameter {name} is not readable")));
    }
    Ok((d.get)(s))
}

/// The value conversion every registry applies before storing `value` in a
/// parameter `name` of kind `kind` (used by [`table_set`],
/// [`table_set_from_input`] and the runtime registry
/// `akantu_model::params::DynamicParameters`, `docs/ARCHITECTURE.md`), so static
/// and runtime-registered parameters accept the same values:
///
/// * a value of kind `kind` is kept as is;
/// * integers are accepted for `Real` (upstream implicit `Int → Real`) and,
///   when in range, for `UInt`;
/// * text is accepted for the byte kinds (`String → ByteString`,
///   `StringSet → ByteStringSet`: lossless, e.g. a Python `str` for an
///   upstream `std::string` parameter);
/// * bytes are accepted for the text kinds only when they are valid UTF-8
///   (`ByteString → String`, `ByteStringSet → StringSet`; otherwise an
///   error, never a lossy replacement).
///
/// # Errors
/// [`Error::InvalidArgument`] for any other combination.
pub fn coerce_to_kind(name: &str, kind: ParamKind, value: ParamValue) -> Result<ParamValue> {
    let text = |b: ByteString| {
        b.into_string().map_err(|b| {
            Error::invalid(format!(
                "Parameter {name} type error, {:?} is not valid UTF-8 and cannot be converted to {kind:?}",
                ByteString::from(b)
            ))
        })
    };
    match (kind, value) {
        #[allow(clippy::cast_precision_loss)] // upstream converts Int → Real implicitly
        (ParamKind::Real, ParamValue::Int(i)) => Ok(ParamValue::Real(i as f64)),
        (ParamKind::UInt, ParamValue::Int(i)) if u32::try_from(i).is_ok() => Ok(ParamValue::Int(i)),
        // Text → bytes is lossless (e.g. a Python `str` for a `std::string`
        // parameter); bytes → text only when they are valid UTF-8.
        (ParamKind::ByteString, ParamValue::String(s)) => Ok(ParamValue::ByteString(s.into())),
        (ParamKind::ByteStringSet, ParamValue::StringSet(s)) => Ok(ParamValue::ByteStringSet(
            s.into_iter().map(ByteString::from).collect(),
        )),
        (ParamKind::String, ParamValue::ByteString(b)) => text(b).map(ParamValue::String),
        (ParamKind::StringSet, ParamValue::ByteStringSet(s)) => s
            .into_iter()
            .map(text)
            .collect::<Result<BTreeSet<String>>>()
            .map(ParamValue::StringSet),
        (k, v) if v.kind() == k => Ok(v),
        (k, v) => Err(Error::invalid(format!(
            "Parameter {name} type error, cannot convert {:?} to {k:?}",
            v.kind()
        ))),
    }
}

/// Table-driven `set_param` (checks `WRITABLE` — not inside
/// [`with_upstream_write_access`], UD-497 — and the value kind; integers
/// are accepted for `Real`, text for the byte kinds, UTF-8 bytes for the
/// text kinds).
pub fn table_set<S>(
    table: &[ParamDescriptor<S>],
    s: &mut S,
    name: &str,
    value: ParamValue,
) -> Result<()> {
    let d = find(table, name)?;
    if !d.access.contains(ParameterAccess::WRITABLE) && !upstream_write_access() {
        return Err(not_writable(name));
    }
    let setter = d
        .set
        .ok_or_else(|| Error::invalid(format!("parameter '{name}' has no setter")))?;
    setter(s, coerce_to_kind(name, d.kind, value)?)
}

/// Table-driven [`ParameterRegistry::param_info`].
pub fn table_info<S>(table: &[ParamDescriptor<S>], name: &str) -> Option<ParamInfo> {
    table.iter().find(|d| d.name == name).map(|d| ParamInfo {
        kind: d.kind,
        access: d.access,
    })
}

/// Table-driven [`ParameterRegistry::set_param_from_input`] (checks
/// `PARSABLE` and the value kind).
pub fn table_set_from_input<S>(
    table: &[ParamDescriptor<S>],
    s: &mut S,
    name: &str,
    value: ParamValue,
) -> Result<()> {
    let d = find(table, name)?;
    if !d.access.contains(ParameterAccess::PARSABLE) {
        return Err(Error::invalid(format!("Parameter {name} is not parsable")));
    }
    let setter = d
        .set
        .ok_or_else(|| Error::invalid(format!("parameter '{name}' has no setter")))?;
    setter(s, coerce_to_kind(name, d.kind, value)?)
}

/// Names of a table, in order.
pub fn table_names<S>(table: &[ParamDescriptor<S>]) -> Vec<&'static str> {
    table.iter().map(|d| d.name).collect()
}

fn find<'t, S>(table: &'t [ParamDescriptor<S>], name: &str) -> Result<&'t ParamDescriptor<S>> {
    table.iter().find(|d| d.name == name).ok_or_else(|| {
        Error::not_found(
            "parameter",
            format!(
                "{name} (Parameter {name} does not exists in this scope; possible parameters are: {})",
                table_names(table).join(", ")
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct Dummy {
        e: f64,
        n: i64,
        ro: f64,
        x135: f64,
        v: Vec<f64>,
    }

    static TABLE: &[ParamDescriptor<Dummy>] = &[
        ParamDescriptor::new(
            "E",
            ParamKind::Real,
            ParameterAccess::PARSMOD,
            "modulus",
            |s| ParamValue::Real(s.e),
            Some(|s, v| {
                s.e = v.as_real()?;
                Ok(())
            }),
        ),
        ParamDescriptor::new(
            "n",
            ParamKind::Int,
            ParameterAccess::MODIFIABLE,
            "count",
            |s| ParamValue::Int(s.n),
            Some(|s, v| {
                s.n = v.as_int()?;
                Ok(())
            }),
        ),
        ParamDescriptor::new(
            "ro",
            ParamKind::Real,
            ParameterAccess::READABLE,
            "read only",
            |s| ParamValue::Real(s.ro),
            None,
        ),
        ParamDescriptor::new(
            "X135",
            ParamKind::Real,
            ParameterAccess::PARSABLE,
            "parsable only",
            |s| ParamValue::Real(s.x135),
            Some(|s, v| {
                s.x135 = v.as_real()?;
                Ok(())
            }),
        ),
        ParamDescriptor::new(
            "v",
            ParamKind::VectorReal,
            ParameterAccess::PARSMOD,
            "vector",
            |s| ParamValue::VectorReal(s.v.clone()),
            Some(|s, v| {
                if let ParamValue::VectorReal(x) = v {
                    s.v = fit_vector(s.v.len(), x)?;
                }
                Ok(())
            }),
        ),
    ];

    impl ParameterRegistry for Dummy {
        fn get_param(&self, name: &str) -> Result<ParamValue> {
            table_get(TABLE, self, name)
        }
        fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
            table_set(TABLE, self, name, value)
        }
        fn param_names(&self) -> Vec<&'static str> {
            table_names(TABLE)
        }
        fn param_info(&self, name: &str) -> Option<ParamInfo> {
            table_info(TABLE, name)
        }
        fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
            table_set_from_input(TABLE, self, name, value)
        }
    }

    impl Parsable for Dummy {
        fn section_type(&self) -> ParserType {
            ParserType::Material
        }
        fn parsable_id(&self) -> &str {
            "dummy"
        }
    }

    #[test]
    fn registry_tables() {
        let mut d = Dummy::default();
        table_set(TABLE, &mut d, "E", ParamValue::Int(3)).unwrap();
        assert_eq!(d.e, 3.0);
        table_set(TABLE, &mut d, "n", ParamValue::Int(5)).unwrap();
        assert_eq!(table_get(TABLE, &d, "n").unwrap(), ParamValue::Int(5));
        assert!(table_set(TABLE, &mut d, "n", ParamValue::Real(1.0)).is_err());
        assert!(table_set(TABLE, &mut d, "ro", ParamValue::Real(1.0)).is_err());
        assert!(table_get(TABLE, &d, "missing").is_err());
        assert!(table_get(TABLE, &d, "X135").is_err());
        assert_eq!(table_names(TABLE), vec!["E", "n", "ro", "X135", "v"]);
        assert!(ParameterAccess::PARSMOD.contains(ParameterAccess::PARSABLE));
        assert!(!ParameterAccess::READABLE.contains(ParameterAccess::WRITABLE));
        assert_eq!(
            ParameterAccess::READABLE | ParameterAccess::WRITABLE,
            ParameterAccess::MODIFIABLE
        );
        assert_eq!(ParameterAccess::PARSMOD.flags_string(), "rwp");
        assert_eq!(ParameterAccess::PARSABLE.flags_string(), "--p");
        assert_eq!(ParameterAccess::INTERNAL.flags_string(), "iii");
    }

    #[test]
    fn value_conversions() {
        assert_eq!(ParamValue::Int(2).as_real().unwrap(), 2.0);
        assert!(ParamValue::Bool(true).as_real().is_err());
        let r = ParamValue::Random(RandomParameter {
            base_value: 1.5,
            distribution: None,
        });
        assert_eq!(r.as_real().unwrap(), 1.5);
        assert_eq!(
            ParamValue::Enum("newton_raphson".into()).as_str().unwrap(),
            "newton_raphson"
        );
        let s = ParserSection::new(ParserType::Global, "global");
        assert_eq!(
            ParamValue::parse(ParamKind::Int, "3.5", &s).unwrap(),
            ParamValue::Int(3)
        );
        assert_eq!(
            ParamValue::parse(ParamKind::UInt, "-1", &s).unwrap(),
            ParamValue::Int(4_294_967_295)
        );
        assert_eq!(
            ParamValue::parse(ParamKind::String, "steel  ", &s).unwrap(),
            ParamValue::String("steel  ".into())
        );
        assert_eq!(
            ParamValue::parse(ParamKind::Enum, "newton_raphson  ", &s).unwrap(),
            ParamValue::Enum("newton_raphson".into())
        );
        match ParamValue::parse(ParamKind::Real, "1 + ", &s) {
            Err(Error::Parse { file, column, .. }) => {
                assert_eq!((file.as_str(), column), ("<string>", 5));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parsable_semantics() {
        let text = "E0 = 2\nmaterial elastic [\n E = E0 * 3\n X135 = 7\n v = [1, 2, 3]\n name = steel\n rules a [ ]\n]\n";
        let p = crate::Parser::parse_str(text, "t.dat").unwrap();
        let sect = &p.root.sub_sections[0];
        let mut d = Dummy {
            v: vec![0.0; 2],
            ..Dummy::default()
        };
        // `name` is unknown and the `rules` sub-section has no parsable.
        let err = d.parse_section(sect, false).unwrap_err();
        assert!(
            err.to_string()
                .contains("No parameter named name registered in dummy."),
            "{err}"
        );
        d.parse_section(sect, true).unwrap();
        assert_eq!((d.e, d.x135), (6.0, 7.0));
        assert_eq!(d.v, vec![1.0, 2.0]);
        assert!(d.get_param("X135").is_err());
        // Wrong section type.
        assert!(d.parse_section(&p.root, true).is_err());
        // Not parsable.
        let p2 = crate::Parser::parse_str("material a [\n n = 1\n]", "t.dat").unwrap();
        let e = d.parse_section(&p2.root.sub_sections[0], true).unwrap_err();
        assert!(e.to_string().contains("Parameter n is not parsable"), "{e}");
        // Too short vector.
        let p3 = crate::Parser::parse_str("material a [\n v = [1]\n]", "t.dat").unwrap();
        assert!(d.parse_section(&p3.root.sub_sections[0], true).is_err());
    }

    #[test]
    fn random_parameter_printing() {
        let r = |d| RandomParameter {
            base_value: 10.0,
            distribution: d,
        };
        assert_eq!(
            r(Some(RandomDistribution::Uniform(0.2, 0.5))).to_string(),
            "10 + uniform [ 2.00000000000000011e-01 5.00000000000000000e-01 ]"
        );
        assert_eq!(
            r(Some(RandomDistribution::Weibull(0.2, 0.5))).to_string(),
            "10 + weibull [ 5.00000000000000000e-01 2.00000000000000011e-01 ]"
        );
        assert_eq!(
            r(None).to_string(),
            "10 + uniform [ 0.00000000000000000e+00 0.00000000000000000e+00 ]"
        );
        assert_eq!(
            r(Some(RandomDistribution::Normal(1.0, 2.0))).to_string(),
            "10 + normal [ 1.00000000000000000e+00 2.00000000000000000e+00 0 ]"
        );
        assert_eq!(
            r(Some(RandomDistribution::ChiSquared(3.0))).to_string(),
            "10 + chi_squared [ 3.00000000000000000e+00 1.50000000000000000e+00 1.00000000000000000e+00 0.00000000000000000e+00 1.00000000000000000e+00 0 ]"
        );
    }

    #[test]
    fn fitting() {
        assert_eq!(fit_vector(0, vec![1.0, 2.0]).unwrap(), vec![1.0, 2.0]);
        assert_eq!(fit_vector(1, vec![1.0, 2.0]).unwrap(), vec![1.0]);
        assert!(fit_vector(3, vec![1.0, 2.0]).is_err());
        let m = MatrixX::from_col_major(2, 2, vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        let f = fit_matrix(1, 2, m.clone()).unwrap();
        assert_eq!(f.as_slice(), &[1.0, 3.0]);
        assert!(fit_matrix(3, 1, m).is_err());
    }
}
