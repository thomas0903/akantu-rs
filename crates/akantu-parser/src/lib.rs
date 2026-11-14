// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/{parser.hh, parser.cc,
// parser_tmpl.hh, parser_real.cc, parser_input_files.cc,
// parser_grammar_tmpl.hh, input_file_parser.hh, algebraic_parser.hh,
// algebraic_parser_x3.{hh,cc}, parameter_registry.{hh,cc},
// parameter_registry_tmpl.hh, parsable.{hh,cc}, cppargparse/},
// src/common/aka_random_generator.hh (parameter side),
// src/common/aka_common.cc (getUserParser, option registration).
// Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! # akantu-parser
//!
//! Input files, parameters and command-line options: the port of upstream's
//! `src/io/parser` and of its command-line parser (`cppargparse`). The crate
//! has no Cargo features.
//!
//! * [`section`]: the parsed input-file tree (`ParserSection`, `ParserType`,
//!   `ParserParameter`), lookups with upstream search scopes
//!   ([`SearchScope`]), typed conversions ([`ParamRef`], [`FromParameter`])
//!   and the [`section::Parser`] entry points (`parse_file`, `parse_str`).
//! * [`grammar`]: the `.dat` grammar (port of Spirit Qi `InputFileGrammar`).
//! * [`expr`]: algebraic expressions: the scalar X3 grammar and the
//!   vector/matrix/random Qi grammar, with Boost.Spirit 1.83 number parsing.
//! * [`param`]: typed parameter values, access flags, the
//!   [`param::ParameterRegistry`] trait (upstream `ParameterRegistry`) and
//!   [`param::Parsable`] (upstream `Parsable`), used by materials, solvers,
//!   models and the Python `getParam`/`setParam` API.
//! * [`stream`]: C++ iostream conversions upstream relies on (integers,
//!   booleans, words, `printself` number formatting), clean-room (D-030).
//! * [`bytes`]: [`ByteString`], the exact bytes of names and values.
//! * [`cli`]: command-line options (upstream `cppargparse`).
//!
//! Upstream has no `include` directive in input files.

#![deny(unsafe_code)] // only `cmath` (libm FFI) opts out, see its SAFETY comments
#![warn(missing_docs)]

pub mod bytes;
pub mod cli;
mod cmath;
pub mod expr;
pub mod grammar;
pub mod param;
mod scope;
pub mod section;
pub mod stream;

pub use bytes::ByteString;
pub use expr::ConversionError;
pub use param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    Parsable, RandomDistribution, RandomParameter,
};
pub use scope::SearchScope;
pub use section::{
    AnonymousIds, FIRST_ANONYMOUS_ID, FromParameter, ParamRef, Parser, ParserParameter,
    ParserSection, ParserType,
};
