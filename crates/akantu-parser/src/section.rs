// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/parser.hh, parser.cc,
// parser_input_files.cc, parser_tmpl.hh, src/common/aka_common.cc
// (getUserParser).
// Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! The parsed input-file tree.
//!
//! Upstream format (excerpt):
//! ```text
//! material elastic [
//!     name = steel
//!     rho = 7800      # density
//!     E   = 2.1e11
//!     nu  = 0.3
//! ]
//! ```
//! A section has a [`ParserType`] (`material`), a name (`elastic`; unnamed
//! sections are called `anonymous_<n>`) and an optional option word.
//! Parameter values are stored as raw text and converted on request to the
//! type the consumer asks for (upstream `ParserParameter::operator T()`), so
//! algebraic expressions are evaluated in context: variables are other
//! parameters of the section or of its ancestors (see [`ParamRef`]).
//!
//! **Iteration order.** `parameters` and `sub_sections` keep file order.
//! Upstream stores parameters in a `std::map` (sorted by name) and
//! sub-sections in a `std::multimap` keyed by type (sorted by
//! [`ParserType`] declaration order, file order within a type); code whose
//! behaviour depends on the order (e.g. which unknown parameter is reported
//! first, material instantiation order across types, printing) must use
//! [`ParserSection::parameters_sorted`] and
//! [`ParserSection::sub_sections_sorted`].
//!
//! **Bytes.** Parameter names and values are [`ByteString`]s holding the
//! exact bytes of the file (upstream `std::string`); conversions to Rust
//! `String`s fail explicitly on invalid UTF-8 instead of replacing bytes.
//!
//! **Depth.** Every operation on the tree (parsing, [`ParserSection::relink`],
//! cloning, comparison, printing, dropping) uses explicit stacks, so trees
//! of any depth are handled without native recursion (upstream recurses and
//! overflows its stack between 5 000 and 20 000 nested sections). `Debug`
//! output stops after [`DEBUG_DEPTH`] levels.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use akantu_core::tensor::MatrixX;
use akantu_core::{Error, Result};

use crate::bytes::ByteString;
use crate::expr::{self, ConversionError};
use crate::param::{ParamKind, ParamValue, RandomParameter};
use crate::scope::{ScopeNode, ScopeRef, SearchScope};
use crate::stream::{self, StreamInt};

/// Section types (upstream `ParserType`, same spellings and order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[allow(missing_docs)] // names mirror upstream `AKANTU_SECTION_TYPES`
pub enum ParserType {
    CohesiveInserter,
    Contact,
    EmbeddedInterface,
    Friction,
    Global,
    Heat,
    IntegrationScheme,
    Material,
    Phasefield,
    ConstitutiveLaw,
    Mesh,
    Model,
    ModelSolver,
    Neighborhood,
    Neighborhoods,
    NonLinearSolver,
    NonLocal,
    Rules,
    Solver,
    TimeStepSolver,
    User,
    WeightFunction,
    ContactDetector,
    ContactResolution,
    NotDefined,
}

impl ParserType {
    /// All section types in upstream order.
    pub const ALL: [ParserType; 25] = [
        Self::CohesiveInserter,
        Self::Contact,
        Self::EmbeddedInterface,
        Self::Friction,
        Self::Global,
        Self::Heat,
        Self::IntegrationScheme,
        Self::Material,
        Self::Phasefield,
        Self::ConstitutiveLaw,
        Self::Mesh,
        Self::Model,
        Self::ModelSolver,
        Self::Neighborhood,
        Self::Neighborhoods,
        Self::NonLinearSolver,
        Self::NonLocal,
        Self::Rules,
        Self::Solver,
        Self::TimeStepSolver,
        Self::User,
        Self::WeightFunction,
        Self::ContactDetector,
        Self::ContactResolution,
        Self::NotDefined,
    ];

    /// Keyword used in input files.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CohesiveInserter => "cohesive_inserter",
            Self::Contact => "contact",
            Self::EmbeddedInterface => "embedded_interface",
            Self::Friction => "friction",
            Self::Global => "global",
            Self::Heat => "heat",
            Self::IntegrationScheme => "integration_scheme",
            Self::Material => "material",
            Self::Phasefield => "phasefield",
            Self::ConstitutiveLaw => "constitutive_law",
            Self::Mesh => "mesh",
            Self::Model => "model",
            Self::ModelSolver => "model_solver",
            Self::Neighborhood => "neighborhood",
            Self::Neighborhoods => "neighborhoods",
            Self::NonLinearSolver => "non_linear_solver",
            Self::NonLocal => "non_local",
            Self::Rules => "rules",
            Self::Solver => "solver",
            Self::TimeStepSolver => "time_step_solver",
            Self::User => "user",
            Self::WeightFunction => "weight_function",
            Self::ContactDetector => "contact_detector",
            Self::ContactResolution => "contact_resolution",
            Self::NotDefined => "not_defined",
        }
    }
}

impl fmt::Display for ParserType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for ParserType {
    type Err = Error;
    /// Exact spelling (upstream `operator>>(std::istream&, ParserType&)`).
    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.name() == s)
            .ok_or_else(|| Error::invalid(format!("'{s}' is not a valid section type")))
    }
}

/// One `name = value` entry. The value is the raw text (leading whitespace
/// removed, trailing whitespace kept, `\`-continued lines concatenated) and
/// is converted lazily to the type requested by the consumer.
#[derive(Clone, Debug, PartialEq)]
pub struct ParserParameter {
    /// Parameter name (the raw text of the key, which may include skipped
    /// white space and comments: `E # c\n` is the name `"E # c"`).
    pub name: ByteString,
    /// Raw value bytes.
    pub value: ByteString,
    /// Source file (for diagnostics; as given to the parser).
    pub file: String,
    /// 1-based line of the parameter name.
    pub line: usize,
    /// 1-based column of the parameter name (tab stops every 4 columns, as
    /// Spirit's `position_iterator`).
    pub column: usize,
}

impl ParserParameter {
    /// New parameter without position information.
    #[must_use]
    pub fn new(name: impl Into<ByteString>, value: impl Into<ByteString>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            file: String::new(),
            line: 0,
            column: 0,
        }
    }

    /// Upstream `operator bool()` (never fails, see [`stream::parse_bool`]).
    #[must_use]
    pub fn to_bool(&self) -> bool {
        stream::parse_bool(&self.value)
    }

    /// Upstream generic `operator T()` for integer `T` (stream extraction,
    /// never fails, see [`stream::parse_int`]).
    #[must_use]
    pub fn to_int<T: StreamInt>(&self) -> T {
        stream::parse_int::<T>(&self.value)
    }

    /// Upstream `operator std::string()` (`sstr >> str`): the first
    /// whitespace-delimited word (bytes). The raw text is
    /// [`ParserParameter::value`].
    #[must_use]
    pub fn to_word(&self) -> ByteString {
        ByteString::from(stream::word_bytes(&self.value))
    }

    /// Upstream `operator std::vector<std::string>()` (bytes).
    #[must_use]
    pub fn to_string_list(&self) -> Vec<ByteString> {
        stream::string_list_bytes(&self.value)
            .into_iter()
            .map(ByteString::from)
            .collect()
    }

    /// Upstream `operator std::set<std::string>()` (bytes, ordered
    /// byte-wise like `std::set<std::string>`).
    #[must_use]
    pub fn to_string_set(&self) -> BTreeSet<ByteString> {
        self.to_string_list().into_iter().collect()
    }

    /// Upstream `printself` (`name: value (file:line:column)`), exact bytes.
    pub fn write_upstream_bytes(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.name);
        out.extend_from_slice(b": ");
        out.extend_from_slice(&self.value);
        out.extend_from_slice(format!(" ({}:{}:{})", self.file, self.line, self.column).as_bytes());
    }

    pub(crate) fn conversion_error(&self, what: &str, e: &ConversionError) -> Error {
        Error::Parse {
            file: self.file.clone(),
            line: self.line,
            column: self.column,
            message: format!(
                "cannot convert parameter '{}' = '{}' to {what}: {}",
                self.name,
                self.value,
                e.message()
            ),
        }
    }
}

impl fmt::Display for ParserParameter {
    /// Upstream `ParserParameter::printself`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} ({}:{}:{})",
            self.name, self.value, self.file, self.line, self.column
        )
    }
}

/// A section of the input file.
pub struct ParserSection {
    /// Section type.
    pub section_type: ParserType,
    /// Section name (`material elastic [` → `"elastic"`).
    pub name: String,
    /// Optional option word (`material elastic opt [` → `"opt"`), else empty.
    pub option: String,
    /// Parameters in file order (names are unique within a section).
    pub parameters: Vec<ParserParameter>,
    /// Sub-sections in file order.
    pub sub_sections: Vec<ParserSection>,
    /// Snapshot of the ancestors' parameters (variable scope).
    parent: Option<Arc<ScopeNode>>,
}

impl ParserSection {
    /// A copy without sub-sections (the ancestor snapshot is shared).
    fn shallow_clone(&self) -> Self {
        Self {
            section_type: self.section_type,
            name: self.name.clone(),
            option: self.option.clone(),
            parameters: self.parameters.clone(),
            sub_sections: Vec::with_capacity(self.sub_sections.len()),
            parent: self.parent.clone(),
        }
    }
}

impl Clone for ParserSection {
    /// Deep copy (iterative); the copies share the ancestor snapshots.
    fn clone(&self) -> Self {
        // (source, copy being built, next child to copy)
        let mut stack: Vec<(&ParserSection, ParserSection, usize)> =
            vec![(self, self.shallow_clone(), 0)];
        loop {
            let Some(top) = stack.last_mut() else {
                unreachable!("the root is popped last");
            };
            if let Some(child) = top.0.sub_sections.get(top.2) {
                top.2 += 1;
                stack.push((child, child.shallow_clone(), 0));
                continue;
            }
            let Some((_, done, _)) = stack.pop() else {
                unreachable!("non-empty stack");
            };
            match stack.last_mut() {
                Some(parent) => parent.1.sub_sections.push(done),
                None => return done,
            }
        }
    }
}

impl Drop for ParserSection {
    /// Iterative drop (no native recursion on deep trees).
    fn drop(&mut self) {
        let mut pending = std::mem::take(&mut self.sub_sections);
        while let Some(mut s) = pending.pop() {
            pending.append(&mut s.sub_sections);
        }
    }
}

impl PartialEq for ParserSection {
    /// Compares the content (type, name, option, parameters,
    /// sub-sections, iteratively); the ancestor scope is not compared.
    fn eq(&self, other: &Self) -> bool {
        let mut pending = vec![(self, other)];
        while let Some((a, b)) = pending.pop() {
            if a.section_type != b.section_type
                || a.name != b.name
                || a.option != b.option
                || a.parameters != b.parameters
                || a.sub_sections.len() != b.sub_sections.len()
            {
                return false;
            }
            pending.extend(a.sub_sections.iter().zip(&b.sub_sections));
        }
        true
    }
}

/// Section levels shown by `Debug` (deeper sub-sections are summarised).
pub const DEBUG_DEPTH: usize = 32;

struct DebugSection<'a>(&'a ParserSection, usize);

impl fmt::Debug for DebugSection<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (s, depth) = (self.0, self.1);
        let mut d = f.debug_struct("ParserSection");
        d.field("section_type", &s.section_type)
            .field("name", &s.name)
            .field("option", &s.option)
            .field("parameters", &s.parameters);
        if depth < DEBUG_DEPTH {
            let subs: Vec<DebugSection<'_>> = s
                .sub_sections
                .iter()
                .map(|c| DebugSection(c, depth + 1))
                .collect();
            d.field("sub_sections", &subs);
        } else {
            d.field("nb_sub_sections", &s.sub_sections.len());
        }
        d.field("has_parent_scope", &s.parent.is_some()).finish()
    }
}

impl fmt::Debug for ParserSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        DebugSection(self, 0).fmt(f)
    }
}

/// A parameter found by a lookup, together with the scope in which its
/// value is evaluated (the section that holds it, which may be an ancestor
/// of the section where the lookup started).
#[derive(Clone, Copy, Debug)]
pub struct ParamRef<'a> {
    param: &'a ParserParameter,
    scope: ScopeRef<'a>,
}

impl<'a> ParamRef<'a> {
    /// The parameter.
    #[must_use]
    pub fn parameter(&self) -> &'a ParserParameter {
        self.param
    }

    /// Name.
    #[must_use]
    pub fn name(&self) -> &'a ByteString {
        &self.param.name
    }

    /// Raw value bytes.
    #[must_use]
    pub fn value(&self) -> &'a ByteString {
        &self.param.value
    }

    /// Upstream `operator Real()` (`Parser::parseReal`, X3 grammar).
    pub fn to_real(&self) -> Result<f64> {
        self.get()
    }

    /// Upstream `operator Vector<Real>()` (`Parser::parseVector`).
    pub fn to_vector(&self) -> Result<Vec<f64>> {
        self.get()
    }

    /// Upstream `operator Matrix<Real>()` (`Parser::parseMatrix`); ragged
    /// rows are zero-padded.
    pub fn to_matrix(&self) -> Result<MatrixX> {
        self.get()
    }

    /// Upstream `operator RandomParameter<Real>()`
    /// (`Parser::parseRandomParameter`).
    pub fn to_random(&self) -> Result<RandomParameter> {
        self.get()
    }

    /// Upstream `operator bool()`.
    #[must_use]
    pub fn to_bool(&self) -> bool {
        self.param.to_bool()
    }

    /// Upstream `operator T()` for integers.
    #[must_use]
    pub fn to_int<T: StreamInt>(&self) -> T {
        self.param.to_int()
    }

    /// Upstream `operator std::string()` (first word, bytes).
    #[must_use]
    pub fn to_word(&self) -> ByteString {
        self.param.to_word()
    }

    /// Typed conversion selected by `kind` (upstream
    /// `ParameterTyped<T>::setAuto`): [`ParamKind::ByteString`] keeps the
    /// raw value bytes, as upstream `std::string`/`ID` parameters do.
    pub fn to_value(&self, kind: ParamKind) -> Result<ParamValue> {
        crate::param::convert(kind, &self.param.value, self.scope)
            .map_err(|e| self.param.conversion_error(&format!("{kind:?}"), &e))
    }

    /// Conversion to any [`FromParameter`] type.
    pub fn get<T: FromParameter>(&self) -> Result<T> {
        T::from_parameter(self).map_err(|e| self.param.conversion_error(T::TYPE_NAME, &e))
    }

    /// Conversion keeping the upstream error class.
    pub fn try_get<T: FromParameter>(&self) -> std::result::Result<T, ConversionError> {
        T::from_parameter(self)
    }

    pub(crate) fn scope(&self) -> ScopeRef<'a> {
        self.scope
    }
}

/// Types a parameter can be converted to (upstream
/// `ParserParameter::operator T()` specialisations).
pub trait FromParameter: Sized {
    /// Name used in error messages.
    const TYPE_NAME: &'static str;
    /// Convert.
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError>;
}

impl FromParameter for f64 {
    const TYPE_NAME: &'static str = "Real";
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        expr::eval_real(p.value(), p.scope())
    }
}

impl FromParameter for Vec<f64> {
    const TYPE_NAME: &'static str = "Vector<Real>";
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        expr::eval_vector(p.value(), p.scope())
    }
}

impl FromParameter for MatrixX {
    const TYPE_NAME: &'static str = "Matrix<Real>";
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        crate::param::matrix_from_rows(&expr::eval_matrix(p.value(), p.scope())?)
    }
}

impl FromParameter for RandomParameter {
    const TYPE_NAME: &'static str = "RandomParameter<Real>";
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        Ok(RandomParameter::from_parsed(&expr::eval_random(
            p.value(),
            p.scope(),
        )?))
    }
}

impl FromParameter for bool {
    const TYPE_NAME: &'static str = "bool";
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        Ok(p.to_bool())
    }
}

/// Decodes text for a Rust `String` target (the explicit text boundary):
/// invalid UTF-8 is an error ([`ConversionError::is_invalid_utf8`]) rather
/// than a silent replacement; the bytes stay available through the
/// [`ByteString`] conversions, which accept every value upstream accepts.
pub(crate) fn utf8(b: ByteString) -> std::result::Result<String, ConversionError> {
    b.into_string().map_err(|bytes| {
        ConversionError::invalid_utf8(
            format!(
                "'{}' is not valid UTF-8 (convert to ByteString to keep the bytes)",
                String::from_utf8_lossy(&bytes)
            ),
            0,
        )
    })
}

impl FromParameter for ByteString {
    const TYPE_NAME: &'static str = "std::string";
    /// The first word (upstream `std::string s = param;`), exact bytes.
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        Ok(p.to_word())
    }
}

impl FromParameter for String {
    const TYPE_NAME: &'static str = "std::string";
    /// The first word (upstream `std::string s = param;`) as Rust text: an
    /// error ([`ConversionError::is_invalid_utf8`]) if it is not valid UTF-8
    /// (text convenience; [`ByteString`] is the upstream-compatible target).
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        utf8(p.to_word())
    }
}

impl FromParameter for Vec<ByteString> {
    const TYPE_NAME: &'static str = "std::vector<std::string>";
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        Ok(p.param.to_string_list())
    }
}

impl FromParameter for Vec<String> {
    const TYPE_NAME: &'static str = "std::vector<std::string>";
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        p.param.to_string_list().into_iter().map(utf8).collect()
    }
}

impl FromParameter for BTreeSet<ByteString> {
    const TYPE_NAME: &'static str = "std::set<std::string>";
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        Ok(p.param.to_string_set())
    }
}

impl FromParameter for BTreeSet<String> {
    const TYPE_NAME: &'static str = "std::set<std::string>";
    fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
        p.param.to_string_set().into_iter().map(utf8).collect()
    }
}

macro_rules! from_param_int {
    ($($t:ty),*) => {$(
        impl FromParameter for $t {
            const TYPE_NAME: &'static str = stringify!($t);
            fn from_parameter(p: &ParamRef<'_>) -> std::result::Result<Self, ConversionError> {
                Ok(p.to_int::<$t>())
            }
        }
    )*};
}
from_param_int!(i8, i16, i32, i64, u8, u16, u32, u64);

impl ParserSection {
    /// New empty section without parent scope.
    #[must_use]
    pub fn new(section_type: ParserType, name: impl Into<String>) -> Self {
        Self {
            section_type,
            name: name.into(),
            option: String::new(),
            parameters: Vec::new(),
            sub_sections: Vec::new(),
            parent: None,
        }
    }

    /// New empty section with an option word.
    #[must_use]
    pub fn with_option(
        section_type: ParserType,
        name: impl Into<String>,
        option: impl Into<String>,
    ) -> Self {
        let mut s = Self::new(section_type, name);
        s.option = option.into();
        s
    }

    pub(crate) fn parent_scope(&self) -> Option<&ScopeNode> {
        self.parent.as_deref()
    }

    /// Rebuild the ancestor-scope snapshots of all sub-sections (needed only
    /// after modifying `parameters`/`sub_sections` by hand; parsing and
    /// [`ParserSection::add_parameter`]/[`ParserSection::add_sub_section`]
    /// do it automatically). Iterative.
    pub fn relink(&mut self) {
        let mut pending: Vec<&mut ParserSection> = vec![self];
        while let Some(s) = pending.pop() {
            let node = Arc::new(ScopeNode {
                parameters: s.parameters.clone(),
                parent: s.parent.clone(),
            });
            for c in &mut s.sub_sections {
                c.parent = Some(Arc::clone(&node));
                pending.push(c);
            }
        }
    }

    /// Upstream `addParameter`: errors if the name is already defined in
    /// this section.
    pub fn add_parameter(&mut self, param: ParserParameter) -> Result<&ParserParameter> {
        if self.parameter(&param.name).is_some() {
            return Err(Error::AlreadyExists {
                kind: "parameter",
                id: format!(
                    "{} (the parameter \"{}\" is already defined in this section)",
                    param.name, param.name
                ),
            });
        }
        self.parameters.push(param);
        self.relink();
        let last = self.parameters.len() - 1;
        Ok(&self.parameters[last])
    }

    /// Upstream `addSubSection` (sections with equal type/name may repeat).
    pub fn add_sub_section(&mut self, section: ParserSection) -> &mut ParserSection {
        self.sub_sections.push(section);
        self.relink();
        let last = self.sub_sections.len() - 1;
        &mut self.sub_sections[last]
    }

    /// Sub-sections of a given type, in file order (upstream
    /// `getSubSections(type)`).
    pub fn sub_sections_of(&self, ty: ParserType) -> impl Iterator<Item = &ParserSection> + '_ {
        self.sub_sections
            .iter()
            .filter(move |s| s.section_type == ty)
    }

    /// All sub-sections in upstream `std::multimap` order: by type
    /// (declaration order of [`ParserType`]), file order within a type
    /// (upstream `getSubSections()`).
    #[must_use]
    pub fn sub_sections_sorted(&self) -> Vec<&ParserSection> {
        let mut v: Vec<&ParserSection> = self.sub_sections.iter().collect();
        v.sort_by_key(|s| s.section_type); // stable
        v
    }

    /// Upstream `getNbSubSections(type)` (`None` = all types).
    #[must_use]
    pub fn nb_sub_sections(&self, ty: Option<ParserType>) -> usize {
        match ty {
            Some(t) => self.sub_sections_of(t).count(),
            None => self.sub_sections.len(),
        }
    }

    /// Parameters in upstream `std::map` order (byte-wise by name; upstream
    /// `getParameters()`).
    #[must_use]
    pub fn parameters_sorted(&self) -> Vec<&ParserParameter> {
        let mut v: Vec<&ParserParameter> = self.parameters.iter().collect();
        v.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        v
    }

    /// Parameter in the current scope (upstream `getParameter(name,
    /// _ppsc_current_scope)` without conversion); `name` is compared
    /// byte-wise.
    #[must_use]
    pub fn parameter(&self, name: impl AsRef<[u8]>) -> Option<&ParserParameter> {
        let name = name.as_ref();
        self.parameters.iter().find(|p| p.name.as_bytes() == name)
    }

    /// Upstream `getParameter(name, ctx)`.
    pub fn get_parameter(
        &self,
        name: impl AsRef<[u8]>,
        search: SearchScope,
    ) -> Result<ParamRef<'_>> {
        let name = name.as_ref();
        ScopeRef::Section(self)
            .lookup(name, search)
            .map(|(param, scope)| ParamRef { param, scope })
            .ok_or_else(|| {
                Error::not_found(
                    "parameter",
                    format!(
                        "{} (not found in the specified context)",
                        String::from_utf8_lossy(name)
                    ),
                )
            })
    }

    /// Upstream `hasParameter(name, ctx)`.
    #[must_use]
    pub fn has_parameter(&self, name: impl AsRef<[u8]>, search: SearchScope) -> bool {
        ScopeRef::Section(self)
            .lookup(name.as_ref(), search)
            .is_some()
    }

    /// Upstream `getParameterValue<T>(name, ctx)`.
    pub fn get_parameter_value<T: FromParameter>(
        &self,
        name: impl AsRef<[u8]>,
        search: SearchScope,
    ) -> Result<T> {
        self.get_parameter(name, search)?.get()
    }

    /// Upstream `getParameter(name, default, ctx)`: `default` if the
    /// parameter is missing or its conversion raises an
    /// `akantu::debug::Exception`; an error for the conversion failures that
    /// upstream does not catch (X3 expectation failures, see
    /// [`ConversionError::is_hard`]).
    ///
    /// Also an error, never the default, when upstream converts the value
    /// but the requested Rust text type cannot hold it
    /// ([`ConversionError::is_invalid_utf8`]: `T = String` for a first word
    /// that is not valid UTF-8). Request [`ByteString`] (or
    /// `Vec<ByteString>`, `BTreeSet<ByteString>`) for upstream's
    /// `std::string` semantics on arbitrary bytes.
    pub fn get_parameter_or<T: FromParameter>(
        &self,
        name: impl AsRef<[u8]>,
        default: T,
        search: SearchScope,
    ) -> Result<T> {
        let Ok(p) = self.get_parameter(name, search) else {
            return Ok(default);
        };
        match T::from_parameter(&p) {
            Ok(v) => Ok(v),
            Err(e) if e.is_hard() || e.is_invalid_utf8() => {
                Err(p.param.conversion_error(T::TYPE_NAME, &e))
            }
            Err(_) => Ok(default),
        }
    }

    /// Upstream `getOption(default)`.
    #[must_use]
    pub fn option_or<'a>(&'a self, default: &'a str) -> &'a str {
        if self.option.is_empty() {
            default
        } else {
            &self.option
        }
    }

    /// Upstream `ParserSection::printself(stream, indent)`, exact bytes
    /// (iterative).
    pub fn write_upstream_bytes(&self, out: &mut Vec<u8>, indent: usize) {
        enum Task<'a> {
            Open(&'a ParserSection, usize),
            CloseSubsections(usize),
            Close(usize),
        }
        let space = |n: usize| " ".repeat(n);
        let mut tasks = vec![Task::Open(self, indent)];
        while let Some(task) = tasks.pop() {
            match task {
                Task::Open(s, ind) => {
                    let sp = space(ind);
                    let opt = if s.option.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", s.option)
                    };
                    out.extend_from_slice(
                        format!("{sp}Section({}) {}{opt} [\n", s.section_type, s.name).as_bytes(),
                    );
                    if !s.parameters.is_empty() {
                        out.extend_from_slice(format!("{sp} Parameters [\n").as_bytes());
                        for p in s.parameters_sorted() {
                            out.extend_from_slice(format!("{sp} + ").as_bytes());
                            p.write_upstream_bytes(out);
                            out.push(b'\n');
                        }
                        out.extend_from_slice(format!("{sp} ]\n").as_bytes());
                    }
                    tasks.push(Task::Close(ind));
                    if !s.sub_sections.is_empty() {
                        out.extend_from_slice(format!("{sp} Subsections [\n").as_bytes());
                        tasks.push(Task::CloseSubsections(ind));
                        for c in s.sub_sections_sorted().into_iter().rev() {
                            tasks.push(Task::Open(c, ind + 2));
                        }
                    }
                }
                Task::CloseSubsections(ind) => {
                    out.extend_from_slice(format!("\n{} ]\n", space(ind)).as_bytes());
                }
                Task::Close(ind) => out.extend_from_slice(format!("{}]\n", space(ind)).as_bytes()),
            }
        }
    }

    /// Upstream `ParserSection::printself(stream, indent)` as text (bytes
    /// that are not UTF-8 are shown lossily; see
    /// [`ParserSection::write_upstream_bytes`]).
    pub fn write_upstream(&self, f: &mut impl fmt::Write, indent: usize) -> fmt::Result {
        let mut out = Vec::new();
        self.write_upstream_bytes(&mut out, indent);
        f.write_str(&String::from_utf8_lossy(&out))
    }
}

impl fmt::Display for ParserSection {
    /// Upstream `operator<<(std::ostream&, const ParserSection&)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_upstream(f, 0)
    }
}

/// First identifier of unnamed sections (`anonymous_12`, …).
pub const FIRST_ANONYMOUS_ID: usize = 12;

/// Upstream `static size_t id = 12;` in `create_subsection`: a
/// process-global counter incremented for **every** section created by the
/// grammar (named or not), across all parsed files.
static ANONYMOUS_COUNTER: AtomicUsize = AtomicUsize::new(FIRST_ANONYMOUS_ID);

/// Numbering of `anonymous_<n>` section names. Both counters wrap around
/// like upstream's `size_t` (`usize::MAX` is followed by `0`).
#[derive(Debug)]
pub enum AnonymousIds<'a> {
    /// The process-global counter (upstream behaviour).
    Global,
    /// A caller-owned counter (e.g. `&mut FIRST_ANONYMOUS_ID` copy to
    /// reproduce a fresh upstream process).
    Local(&'a mut usize),
}

impl AnonymousIds<'_> {
    pub(crate) fn next(&mut self) -> usize {
        match self {
            // `fetch_add` wraps on overflow.
            AnonymousIds::Global => ANONYMOUS_COUNTER.fetch_add(1, AtomicOrdering::Relaxed),
            AnonymousIds::Local(c) => {
                let v = **c;
                **c = v.wrapping_add(1);
                v
            }
        }
    }
}

/// A parsed input file: the root is the `global` section named `"global"`.
#[derive(Clone, Debug, PartialEq)]
pub struct Parser {
    /// Root section (`ParserType::Global`, name `"global"`).
    pub root: ParserSection,
    /// Upstream `getLastParsedFile()`.
    pub last_parsed_file: String,
    /// Upstream `Parser::permissive_parser` (process-global static upstream;
    /// see [`Parser::is_permissive`]).
    permissive: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            root: ParserSection::new(ParserType::Global, "global"),
            last_parsed_file: String::new(),
            permissive: false,
        }
    }
}

impl Parser {
    /// Parse an input file (upstream `Parser::parse(filename)` on a fresh
    /// parser). Upstream has no `include` mechanism.
    pub fn parse_file(path: impl AsRef<Path>) -> Result<Self> {
        let mut p = Self::default();
        p.parse(path)?;
        Ok(p)
    }

    /// As [`Parser::parse_file`] with explicit anonymous-section numbering.
    pub fn parse_file_with(path: impl AsRef<Path>, ids: AnonymousIds<'_>) -> Result<Self> {
        let mut p = Self::default();
        p.parse_with(path, ids)?;
        Ok(p)
    }

    /// Parse input text (for tests and Python `parseInput` from strings);
    /// `file_name` is used in positions and messages.
    pub fn parse_str(text: &str, file_name: &str) -> Result<Self> {
        Self::parse_bytes(text.as_bytes(), file_name, AnonymousIds::Global)
    }

    /// As [`Parser::parse_str`] with explicit anonymous-section numbering.
    pub fn parse_str_with(text: &str, file_name: &str, ids: AnonymousIds<'_>) -> Result<Self> {
        Self::parse_bytes(text.as_bytes(), file_name, ids)
    }

    /// Parse raw bytes (names, values and comments may contain any bytes;
    /// they are kept exactly, see [`ByteString`]).
    pub fn parse_bytes(bytes: &[u8], file_name: &str, ids: AnonymousIds<'_>) -> Result<Self> {
        let mut p = Self::default();
        p.parse_bytes_into(bytes, file_name, ids)?;
        Ok(p)
    }

    /// Upstream member `Parser::parse(filename)`: clears the tree **before**
    /// opening the file (so a file that cannot be read leaves an empty
    /// tree), parses the file into it (on error the partially filled tree is
    /// kept, as upstream), updates the permissive flag only if the file sets
    /// `permissive_parser`, and records the file name on success. The flag
    /// and the last file name are kept on errors.
    pub fn parse(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.parse_with(path, AnonymousIds::Global)
    }

    /// As [`Parser::parse`] with explicit anonymous-section numbering.
    pub fn parse_with(&mut self, path: impl AsRef<Path>, ids: AnonymousIds<'_>) -> Result<()> {
        let path = path.as_ref();
        let name = path.to_string_lossy().into_owned();
        self.clean();
        let bytes = std::fs::read(path).map_err(|source| Error::Io {
            path: name.clone(),
            source,
        })?;
        self.parse_bytes_into(&bytes, &name, ids)
    }

    /// Upstream `ParserSection::clean()` on the root.
    fn clean(&mut self) {
        self.root.parameters.clear();
        self.root.sub_sections.clear();
    }

    fn parse_bytes_into(
        &mut self,
        bytes: &[u8],
        file_name: &str,
        mut ids: AnonymousIds<'_>,
    ) -> Result<()> {
        self.clean();
        let r = crate::grammar::parse_into(&mut self.root, bytes, file_name, &mut ids);
        self.root.relink();
        r?;
        if let Some(p) = self.root.parameter("permissive_parser") {
            self.permissive = p.to_bool();
        }
        self.last_parsed_file = file_name.to_string();
        Ok(())
    }

    /// Upstream `Parser::isPermissive()`. Upstream the flag is a
    /// process-global static shared by all parsers; here it belongs to the
    /// parser (the global parser of `akantu::runtime` reproduces the
    /// upstream behaviour).
    #[must_use]
    pub fn is_permissive(&self) -> bool {
        self.permissive
    }

    /// Upstream `Parser::setPermissive()`.
    pub fn set_permissive(&mut self, permissive: bool) {
        self.permissive = permissive;
    }

    /// Upstream `getLastParsedFile()`.
    #[must_use]
    pub fn last_parsed_file(&self) -> &str {
        &self.last_parsed_file
    }

    /// Upstream `getUserParser()`: the first `user` section (upstream
    /// dereferences an end iterator when there is none; here an error).
    pub fn user_section(&self) -> Result<&ParserSection> {
        self.root
            .sub_sections_of(ParserType::User)
            .next()
            .ok_or_else(|| Error::not_found("section", "user"))
    }

    /// Look up `name` starting in the section reached by `path` (indices into
    /// `sub_sections` from the root), optionally walking up to the root
    /// (upstream `_ppsc_current_and_parent_scope`).
    #[must_use]
    pub fn find_parameter(
        &self,
        path: &[usize],
        name: impl AsRef<[u8]>,
        search_parents: bool,
    ) -> Option<&ParserParameter> {
        let name = name.as_ref();
        let mut chain = vec![&self.root];
        for &i in path {
            let next = chain.last()?.sub_sections.get(i)?;
            chain.push(next);
        }
        let mut it = chain.iter().rev();
        if search_parents {
            it.find_map(|s| s.parameter(name))
        } else {
            it.next().and_then(|s| s.parameter(name))
        }
    }
}

impl fmt::Display for Parser {
    /// Upstream `operator<<` on the parser (its root section).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.root.write_upstream(f, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, value: &str) -> ParserParameter {
        ParserParameter {
            name: name.into(),
            value: value.into(),
            file: "t.dat".into(),
            line: 1,
            column: 1,
        }
    }

    #[test]
    fn section_types_roundtrip() {
        for t in ParserType::ALL {
            assert_eq!(t.name().parse::<ParserType>().unwrap(), t);
        }
        assert!("materials".parse::<ParserType>().is_err());
    }

    #[test]
    fn scoped_lookup() {
        let mut parser = Parser::default();
        parser.root.parameters.push(p("young", "2e11"));
        let mut mat = ParserSection::new(ParserType::Material, "elastic");
        mat.parameters.push(p("rho", "7800"));
        mat.parameters.push(p("e2", "2 * young"));
        parser.root.add_sub_section(mat);
        assert_eq!(
            parser.find_parameter(&[0], "rho", false).unwrap().value,
            "7800"
        );
        assert!(parser.find_parameter(&[0], "young", false).is_none());
        assert_eq!(
            parser.find_parameter(&[0], "young", true).unwrap().value,
            "2e11"
        );
        assert!(parser.find_parameter(&[3], "rho", true).is_none());
        assert_eq!(parser.root.sub_sections_of(ParserType::Material).count(), 1);
        let mat = &parser.root.sub_sections[0];
        assert!(mat.get_parameter("young", SearchScope::Current).is_err());
        assert!(mat.has_parameter("young", SearchScope::CurrentAndParent));
        assert!(mat.has_parameter("young", SearchScope::Parent));
        assert!(!mat.has_parameter("rho", SearchScope::Parent));
        let e2: f64 = mat.get_parameter_value("e2", SearchScope::Current).unwrap();
        assert_eq!(e2, 4e11);
        assert_eq!(
            mat.get_parameter_or("missing", 1.5, SearchScope::Current)
                .unwrap(),
            1.5
        );
    }
}
