// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/cppargparse/cppargparse.hh,
// cppargparse.cc, cppargparse_tmpl.hh, src/common/aka_common.cc
// (initialize: registration of the default options).
// Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! Command-line options (upstream `cppargparse`, `getStaticArgumentParser`).
//!
//! A Python-`argparse`-like parser: optional arguments (`-f`, `--foo`,
//! `-f;--foo`) and positional arguments, `nargs` (`N`, `?`, `+`, `*`), typed
//! values (string, integer, float, boolean), defaults and `const` values,
//! a built-in `-h;--help`, and upstream's parse flags
//! (`_stop_on_not_parsed`, `_remove_parsed`). Upstream `initialize(argc,
//! argv)` registers the options of [`register_akantu_options`] and parses
//! with `_remove_parsed` only, so unknown arguments are left to the
//! application.
//!
//! Differences: upstream prints the error and the help text (on rank 0) and
//! calls `exit()`; this library returns the message as an
//! [`Error::InvalidArgument`] (and [`ParseOutcome::HelpRequested`] for
//! `--help`) and lets the caller print [`ArgumentParser::help_string`] and
//! exit.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use akantu_core::{Error, Result};

use crate::stream;

/// Kind of an argument (derived from its name and `nargs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentKind {
    /// `--flag` (optional argument with `nargs = 0`)
    Flag,
    /// `--name value…` (optional argument taking values)
    Value,
    /// positional
    Positional,
}

/// Upstream `ArgumentType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentType {
    /// `_string`
    String,
    /// `_integer` (`long int`)
    Integer,
    /// `_float` (`double`)
    Float,
    /// `_boolean` (read as a number: `0`/`1`)
    Boolean,
}

/// Upstream `nargs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nargs {
    /// `N` values (`0` for flags).
    Exactly(usize),
    /// `_one_if_possible` (`?`).
    OneIfPossible,
    /// `_at_least_one` (`+`).
    AtLeastOne,
    /// `_any` (`*`).
    Any,
}

/// Upstream `ParseFlags`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseFlags {
    /// `_stop_on_not_parsed`: unknown arguments are an error.
    pub stop_on_not_parsed: bool,
    /// `_remove_parsed`: remove consumed arguments (except `--help`) from
    /// `argv`.
    pub remove_parsed: bool,
}

impl ParseFlags {
    /// `_no_flags`
    pub const NO_FLAGS: Self = Self {
        stop_on_not_parsed: false,
        remove_parsed: false,
    };
    /// `_stop_on_not_parsed` (upstream default of `parse`).
    pub const STOP_ON_NOT_PARSED: Self = Self {
        stop_on_not_parsed: true,
        remove_parsed: false,
    };
    /// `_remove_parsed` (used by upstream `initialize`).
    pub const REMOVE_PARSED: Self = Self {
        stop_on_not_parsed: false,
        remove_parsed: true,
    };
}

impl Default for ParseFlags {
    fn default() -> Self {
        Self::STOP_ON_NOT_PARSED
    }
}

/// A typed argument value.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgValue {
    /// String.
    String(String),
    /// Integer (`long int`).
    Integer(i64),
    /// Float (`double`).
    Float(f64),
    /// Boolean.
    Boolean(bool),
}

impl fmt::Display for ArgValue {
    /// `std::ostream <<` with `std::boolalpha`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => f.write_str(s),
            Self::Integer(i) => write!(f, "{i}"),
            Self::Float(x) => f.write_str(&stream::format_double(*x)),
            Self::Boolean(b) => write!(f, "{b}"),
        }
    }
}

/// Declared argument (upstream `addArgument(name_or_flag, help, nargs,
/// type[, default[, const]])`).
#[derive(Clone, Debug, PartialEq)]
pub struct ArgumentSpec {
    /// Name or flags in upstream syntax: `"-f"`, `"--foo"`, `"-f;--foo"`
    /// or a positional name `"bar"`.
    pub name: String,
    /// Help text.
    pub help: String,
    /// Number of values.
    pub nargs: Nargs,
    /// Value type.
    pub arg_type: ArgumentType,
    /// Default value (text, converted like a command-line value).
    pub default: Option<String>,
    /// `const` value (used when an optional argument with `nargs` `?` or
    /// `0` is given without value).
    pub const_value: Option<String>,
}

impl ArgumentSpec {
    /// One string value, no default.
    #[must_use]
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            nargs: Nargs::Exactly(1),
            arg_type: ArgumentType::String,
            default: None,
            const_value: None,
        }
    }

    /// Set `nargs`.
    #[must_use]
    pub fn nargs(mut self, nargs: Nargs) -> Self {
        self.nargs = nargs;
        self
    }

    /// Set the type.
    #[must_use]
    pub fn arg_type(mut self, t: ArgumentType) -> Self {
        self.arg_type = t;
        self
    }

    /// Set the default.
    #[must_use]
    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default = Some(v.into());
        self
    }

    /// Set the `const` value.
    #[must_use]
    pub fn const_value(mut self, v: impl Into<String>) -> Self {
        self.const_value = Some(v.into());
        self
    }

    /// Kind implied by the name and `nargs`.
    #[must_use]
    pub fn kind(&self) -> ArgumentKind {
        if !self.name.starts_with('-') {
            ArgumentKind::Positional
        } else if self.nargs == Nargs::Exactly(0) {
            ArgumentKind::Flag
        } else {
            ArgumentKind::Value
        }
    }
}

#[derive(Clone, Debug)]
struct Argument {
    spec: ArgumentSpec,
    name: String,
    keys: Vec<String>,
    is_positional: bool,
    required: bool,
    /// Upstream `Argument_::parsed` (given on the command line).
    parsed: bool,
    /// Member of upstream `success_parsed` (given or defaulted).
    success: bool,
    raw: Vec<String>,
    values: Vec<ArgValue>,
}

/// Result of [`ArgumentParser::parse_argv`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseOutcome {
    /// Arguments parsed.
    Parsed,
    /// `-h`/`--help` was given (upstream prints the help and exits with
    /// status 0).
    HelpRequested,
}

/// Argument parser (upstream `cppargparse::ArgumentParser`).
#[derive(Clone, Debug)]
pub struct ArgumentParser {
    /// Arguments by name (upstream `std::map`, sorted by name).
    arguments: BTreeMap<String, Argument>,
    /// Option key (`-f`, `--foo`) → argument name.
    key_args: BTreeMap<String, String>,
    /// Positional argument names in declaration order.
    pos_args: Vec<String>,
    /// Upstream `success_parsed` entries that still point to a replaced
    /// declaration: re-declaring an argument (`addArgument` with a name
    /// already declared) replaces `arguments[name]` but not
    /// `success_parsed[name]`, so the old value stays visible until the new
    /// declaration is parsed or defaulted.
    stale: BTreeMap<String, Argument>,
    program_name: String,
    prank: i32,
}

impl Default for ArgumentParser {
    /// Upstream constructor: registers `-h;--help`.
    fn default() -> Self {
        let mut p = Self {
            arguments: BTreeMap::new(),
            key_args: BTreeMap::new(),
            pos_args: Vec::new(),
            stale: BTreeMap::new(),
            program_name: String::new(),
            prank: 0,
        };
        let help = ArgumentSpec::new("-h;--help", "show this help message and exit")
            .nargs(Nargs::Exactly(0))
            .arg_type(ArgumentType::Boolean)
            .default_value("false")
            .const_value("true");
        // The built-in specification is valid by construction.
        if p.add_argument(help).is_err() {
            debug_assert!(false, "invalid built-in help argument");
        }
        p
    }
}

/// Upstream `checkType`: does `sstr >> T` leave `failbit` clear? (`T` is
/// `std::string`, `double`, `long int` or `bool` with `noboolalpha`.) For
/// floats this rejects overflow (`1e400` → `failbit`), incomplete exponents
/// (`1e`), `nan`/`inf` and empty values, and accepts underflow (`1e-400`) and
/// valid prefixes (`1e5x`, `0x10`), see [`stream::extract_double`].
fn check_type(t: ArgumentType, v: &str) -> bool {
    let b = v.as_bytes();
    match t {
        ArgumentType::String => stream::extract_word(b, 0).is_some(),
        ArgumentType::Float => !stream::extract_double(b, 0).fail,
        ArgumentType::Integer => !stream::extract_int::<i64>(b, 0).fail,
        ArgumentType::Boolean => !stream::extract_bool(b, 0, false).fail,
    }
}

/// Upstream `ArgumentStorage<T>::addValue` (`T t; sstr >> t;`; strings are
/// kept verbatim). Upstream's `t` is uninitialised when nothing is stored
/// (blank text, possible for defaults only); the port stores zero/`false`.
fn convert_value(t: ArgumentType, v: &str) -> ArgValue {
    let b = v.as_bytes();
    match t {
        ArgumentType::String => ArgValue::String(v.to_string()),
        ArgumentType::Integer => ArgValue::Integer(stream::parse_int::<i64>(b)),
        ArgumentType::Float => ArgValue::Float(stream::extract_double(b, 0).value.unwrap_or(0.0)),
        ArgumentType::Boolean => {
            ArgValue::Boolean(stream::extract_bool(b, 0, false).value.unwrap_or(false))
        }
    }
}

impl ArgumentParser {
    /// New parser with the built-in `-h;--help`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `setParallelContext`: help is printed only on rank 0 (the
    /// caller decides; [`ArgumentParser::should_print`] reports it).
    pub fn set_parallel_context(&mut self, prank: i32, psize: i32) {
        let _ = psize;
        self.prank = prank;
    }

    /// `true` on rank 0 (where upstream prints messages and help).
    #[must_use]
    pub fn should_print(&self) -> bool {
        self.prank == 0
    }

    /// Declare an argument (upstream `addArgument`). Errors on an invalid
    /// name syntax (upstream `std::invalid_argument`). Re-declaring a name
    /// replaces the previous declaration, as upstream; a value the previous
    /// declaration was given (or defaulted to) stays visible to
    /// [`ArgumentParser::has`] and the getters until the new declaration is
    /// parsed or defaulted (upstream `success_parsed` keeps pointing to the
    /// old declaration). `akantu::runtime::initialize` relies on this for
    /// repeated calls, as upstream. Difference (UD-502): a key the new
    /// declaration drops still reaches the argument of that name, i.e. the
    /// new declaration (upstream: the replaced one).
    pub fn add_argument(&mut self, spec: ArgumentSpec) -> Result<()> {
        let keys: Vec<&str> = spec.name.split(';').collect();
        let mut long_key = None;
        let mut short_key = None;
        let mut problem = keys.len() > 2 || spec.name.is_empty();
        for (i, k) in keys.iter().enumerate() {
            if k.starts_with("--") {
                problem |= long_key.is_some();
                long_key = Some(i);
            } else if k.starts_with('-') {
                problem |= long_key.is_some();
                short_key = Some(i);
            }
        }
        problem |= keys.len() == 2 && (long_key.is_none() || short_key.is_none());
        if problem {
            return Err(Error::invalid(
                "Synthax of name or flags is not correct. Possible synthax are '-f', '-f;--foo', '--foo', 'bar'",
            ));
        }
        let (name, is_positional) = if let Some(l) = long_key {
            (keys[l][2..].to_string(), false)
        } else if let Some(s) = short_key {
            (keys[s][1..].to_string(), false)
        } else {
            (keys[0].to_string(), true)
        };
        let mut arg_keys = Vec::new();
        if !is_positional {
            if let Some(s) = short_key {
                arg_keys.push(keys[s].to_string());
            }
            if let Some(l) = long_key {
                arg_keys.push(keys[l].to_string());
            }
        }
        for k in &arg_keys {
            self.key_args.insert(k.clone(), name.clone());
        }
        if is_positional {
            self.pos_args.push(name.clone());
        }
        let required = is_positional && spec.nargs != Nargs::OneIfPossible;
        if let Some(old) = self.arguments.remove(&name)
            && old.success
        {
            self.stale.insert(name.clone(), old);
        }
        self.arguments.insert(
            name.clone(),
            Argument {
                spec,
                name,
                keys: arg_keys,
                is_positional,
                required,
                parsed: false,
                success: false,
                raw: Vec::new(),
                values: Vec::new(),
            },
        );
        Ok(())
    }

    /// Parse `args` (**without** the program name) with upstream's default
    /// flags (`_stop_on_not_parsed`). `--help` is not an error: check
    /// [`ArgumentParser::has`]`("help")`.
    pub fn parse(&mut self, args: &[String]) -> Result<()> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(String::new());
        argv.extend_from_slice(args);
        self.parse_argv(&mut argv, ParseFlags::STOP_ON_NOT_PARSED, true)
            .map(|_| ())
    }

    /// Upstream `parse(argc, argv, flags, parse_help)`: `argv[0]` is the
    /// program name; with `remove_parsed` the consumed arguments are removed
    /// from `argv`.
    pub fn parse_argv(
        &mut self,
        argv: &mut Vec<String>,
        flags: ParseFlags,
        parse_help: bool,
    ) -> Result<ParseOutcome> {
        let argvs = argv.clone();
        let argc = argvs.len();
        if self.program_name.is_empty()
            && let Some(p) = argvs.first()
        {
            self.program_name = p
                .rsplit('/')
                .find(|s| !s.is_empty())
                .unwrap_or("")
                .to_string();
        }
        let mut positional_queue: VecDeque<String> = self.pos_args.iter().cloned().collect();
        let mut to_remove: Vec<usize> = Vec::new();
        let mut current = 1usize;
        while current < argc {
            let arg = argvs[current].clone();
            current += 1;
            let (name, is_positional) = match self.key_args.get(&arg) {
                Some(n) => (n.clone(), false),
                None => {
                    let Some(front) = positional_queue.front() else {
                        if flags.stop_on_not_parsed {
                            return Err(Error::invalid(format!("Argument {arg} not recognized")));
                        }
                        continue;
                    };
                    current -= 1;
                    (front.clone(), true)
                }
            };
            if flags.remove_parsed && !is_positional && name != "help" {
                to_remove.push(current - 1);
            }
            let Some(argument) = self.arguments.get(&name) else {
                continue;
            };
            let arg_type = argument.spec.arg_type;
            let nargs = argument.spec.nargs;
            let remaining = argc - current;
            let (min, max) = match nargs {
                Nargs::OneIfPossible => (0, 1),
                Nargs::AtLeastOne => (1, remaining),
                Nargs::Any => (0, remaining),
                Nargs::Exactly(n) => (n, n),
            };
            let mut values = Vec::new();
            let mut consumed = 0usize;
            if max <= remaining {
                while consumed < max {
                    let v = argvs[current].clone();
                    current += 1;
                    let is_key = self.key_args.contains_key(&v);
                    if !is_key && check_type(arg_type, &v) {
                        values.push(v);
                        if flags.remove_parsed {
                            to_remove.push(current - 1);
                        }
                        consumed += 1;
                    } else {
                        if !is_positional || is_key {
                            current -= 1;
                        }
                        break;
                    }
                }
            }
            if consumed < min {
                if !is_positional {
                    return Err(Error::invalid(format!(
                        "Not enought values for the argument {name} where provided"
                    )));
                }
                if flags.stop_on_not_parsed {
                    return Err(Error::invalid(format!("Argument {arg} not recognized")));
                }
                continue;
            }
            if is_positional {
                positional_queue.pop_front();
            }
            let Some(argument) = self.arguments.get_mut(&name) else {
                continue;
            };
            if argument.parsed {
                return Err(Error::invalid(format!(
                    "Argument {name} already present in the list of argument"
                )));
            }
            argument.parsed = true;
            argument.success = true;
            self.stale.remove(&name);
            if matches!(nargs, Nargs::OneIfPossible | Nargs::Exactly(0)) && consumed == 0 {
                if let Some(c) = argument.spec.const_value.clone() {
                    argument.set_to(&c);
                } else if let Some(d) = argument.spec.default.clone() {
                    argument.set_to(&d);
                }
            } else {
                for v in values {
                    argument.values.push(convert_value(arg_type, &v));
                    argument.raw.push(v);
                }
            }
        }
        for argument in self.arguments.values_mut() {
            if !argument.parsed {
                if let Some(d) = argument.spec.default.clone() {
                    argument.set_to(&d);
                    argument.success = true;
                    self.stale.remove(&argument.name);
                }
                if argument.required {
                    return Err(Error::invalid(format!(
                        "Argument {} required but not given!",
                        argument.name
                    )));
                }
            }
        }
        if !to_remove.is_empty() {
            let mut i = 0;
            argv.retain(|_| {
                let keep = !to_remove.contains(&i);
                i += 1;
                keep
            });
        }
        let help = self.arguments.get("help").is_some_and(|a| a.parsed);
        if help && parse_help {
            return Ok(ParseOutcome::HelpRequested);
        }
        Ok(ParseOutcome::Parsed)
    }

    /// Upstream `success_parsed[name]` if present (the current declaration,
    /// else a replaced one that was given or defaulted), else the current
    /// declaration.
    fn lookup(&self, name: &str) -> Option<&Argument> {
        let key = name.trim_start_matches('-');
        match self.arguments.get(key) {
            Some(a) if a.success => Some(a),
            current => self.stale.get(key).or(current),
        }
    }

    /// Upstream `has(name)`: the argument was given or has a default
    /// (`name` with or without leading dashes).
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.lookup(name).is_some_and(|a| a.success)
    }

    /// Raw text of the (first) value of an argument, if given or defaulted.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.lookup(name)
            .filter(|a| a.success)
            .and_then(|a| a.raw.first())
            .map(String::as_str)
    }

    /// All typed values (upstream `operator[]` cast to `std::vector<T>`).
    pub fn values(&self, name: &str) -> Result<&[ArgValue]> {
        match self.lookup(name) {
            Some(a) if a.success => Ok(&a.values),
            _ => Err(Error::not_found(
                "argument",
                format!(
                    "{name} (no argument named '{name}' was found in the parsed argument, make sur to specify it 'required' or to give it a default value)"
                ),
            )),
        }
    }

    /// The single typed value (upstream `operator[]` cast to `T`: an error
    /// unless exactly one value is present).
    pub fn value(&self, name: &str) -> Result<&ArgValue> {
        match self.values(name)? {
            [v] => Ok(v),
            _ => Err(Error::invalid(format!(
                "Not enougth or too many argument where passed for the command line argument: {}",
                name.trim_start_matches('-')
            ))),
        }
    }

    /// String value.
    pub fn get_string(&self, name: &str) -> Result<String> {
        match self.value(name)? {
            ArgValue::String(s) => Ok(s.clone()),
            other => Err(Error::invalid(format!(
                "argument {name} is not a string: {other:?}"
            ))),
        }
    }

    /// Integer value.
    pub fn get_int(&self, name: &str) -> Result<i64> {
        match self.value(name)? {
            ArgValue::Integer(i) => Ok(*i),
            other => Err(Error::invalid(format!(
                "argument {name} is not an integer: {other:?}"
            ))),
        }
    }

    /// Float value.
    pub fn get_float(&self, name: &str) -> Result<f64> {
        match self.value(name)? {
            ArgValue::Float(x) => Ok(*x),
            other => Err(Error::invalid(format!(
                "argument {name} is not a float: {other:?}"
            ))),
        }
    }

    /// Boolean value.
    pub fn get_bool(&self, name: &str) -> Result<bool> {
        match self.value(name)? {
            ArgValue::Boolean(b) => Ok(*b),
            other => Err(Error::invalid(format!(
                "argument {name} is not a boolean: {other:?}"
            ))),
        }
    }

    /// Upstream `print_usage`.
    #[must_use]
    pub fn usage_string(&self) -> String {
        let mut s = format!("Usage: {}", self.program_name);
        for a in self.arguments.values().filter(|a| !a.is_positional) {
            if !a.required {
                s.push_str(" [");
            }
            s.push_str(a.keys.first().map_or("", String::as_str));
            s.push_str(&usage_nargs(a));
            if !a.required {
                s.push(']');
            }
        }
        for n in &self.pos_args {
            if let Some(a) = self.arguments.get(n) {
                s.push_str(&usage_nargs(a));
            }
        }
        s.push('\n');
        s
    }

    /// Upstream `print_help`.
    #[must_use]
    pub fn help_string(&self) -> String {
        let mut s = self.usage_string();
        if !self.pos_args.is_empty() {
            s.push_str("\npositional arguments:\n");
            for n in &self.pos_args {
                if let Some(a) = self.arguments.get(n) {
                    s.push_str(&help_argument(a));
                }
            }
        }
        if !self.key_args.is_empty() {
            s.push_str("\noptional arguments:\n");
            for a in self.arguments.values().filter(|a| !a.is_positional) {
                s.push_str(&help_argument(a));
            }
        }
        s
    }
}

impl fmt::Display for ArgumentParser {
    /// Upstream `printself`: `name = v1 v2 …` per parsed argument.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for a in self.arguments.values().filter(|a| a.success) {
            write!(f, "{} =", a.name)?;
            for v in &a.values {
                write!(f, " {v}")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

impl Argument {
    /// `setToDefault`/`setToConst`.
    fn set_to(&mut self, text: &str) {
        self.raw = vec![text.to_string()];
        self.values = vec![match self.spec.arg_type {
            ArgumentType::Boolean => ArgValue::Boolean(text == "true" || text == "1"),
            t => convert_value(t, text),
        }];
    }
}

fn usage_nargs(a: &Argument) -> String {
    let u = a.name.to_uppercase();
    match a.spec.nargs {
        Nargs::OneIfPossible => format!(" [{u}]"),
        Nargs::AtLeastOne => format!(" {u} [{u} ...]"),
        Nargs::Any => format!(" [{u} ...]"),
        Nargs::Exactly(n) => format!(" {u}").repeat(n),
    }
}

fn help_argument(a: &Argument) -> String {
    let key = if a.is_positional {
        a.name.clone()
    } else {
        a.keys
            .iter()
            .map(|k| format!("{k}{}", usage_nargs(a)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut s = format!("  {key:<15}  {}", a.spec.help);
    let shown = |t: &str| match a.spec.arg_type {
        ArgumentType::Boolean => (t == "true" || t == "1").to_string(),
        ty => convert_value(ty, t).to_string(),
    };
    if let Some(d) = &a.spec.default {
        s.push_str(&format!(" (default: {})", shown(d)));
    }
    if let Some(c) = &a.spec.const_value {
        s.push_str(&format!(" (const: {})", shown(c)));
    }
    s.push('\n');
    s
}

/// Upstream `dblWarning`, the default of `--aka_debug_level`.
pub const DEFAULT_DEBUG_LEVEL: i64 = 4;

/// Options registered by upstream `initialize(argc, argv)` (to be parsed
/// with [`ParseFlags::REMOVE_PARSED`]): `--aka_input_file` (string, default
/// empty), `--aka_debug_level` (integer, default 4 = warnings),
/// `--aka_print_backtrace` (flag) and `--aka_seed` (integer, no default).
pub fn register_akantu_options(parser: &mut ArgumentParser) -> Result<()> {
    parser.add_argument(
        ArgumentSpec::new("--aka_input_file", "Akantu's input file").default_value(""),
    )?;
    parser.add_argument(
        ArgumentSpec::new(
            "--aka_debug_level",
            "Akantu's overall debug level (0: error, 1: exceptions, 4: warnings, 5: info, ..., 100: dump more info on levels can be found in aka_error.hh)",
        )
        .arg_type(ArgumentType::Integer)
        .default_value(DEFAULT_DEBUG_LEVEL.to_string()),
    )?;
    parser.add_argument(
        ArgumentSpec::new(
            "--aka_print_backtrace",
            "Should Akantu print a backtrace in case of error",
        )
        .nargs(Nargs::Exactly(0))
        .arg_type(ArgumentType::Boolean)
        .default_value("false")
        .const_value("true"),
    )?;
    parser.add_argument(
        ArgumentSpec::new("--aka_seed", "The seed to use on prank 0")
            .arg_type(ArgumentType::Integer),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    /// Upstream `addArgument` on a declared name
    /// replaces `arguments[name]` but leaves `success_parsed[name]` on the
    /// old declaration (cppargparse.cc:111-205, :337-364): a given value
    /// survives the re-declaration until the new declaration is given or
    /// defaulted; a re-declared argument without default and not given
    /// keeps the old value.
    #[test]
    fn redeclared_argument_keeps_the_old_success_entry() {
        let mut p = ArgumentParser::default();
        let seed = || ArgumentSpec::new("--seed", "s").arg_type(ArgumentType::Integer);
        let level = || {
            ArgumentSpec::new("--level", "l")
                .arg_type(ArgumentType::Integer)
                .default_value("4")
        };
        p.add_argument(seed()).unwrap();
        p.add_argument(level()).unwrap();
        let mut a = args(&["p", "--seed", "77", "--level", "9"]);
        p.parse_argv(&mut a, ParseFlags::REMOVE_PARSED, true)
            .unwrap();
        assert_eq!(
            (p.get_int("seed").unwrap(), p.get_int("level").unwrap()),
            (77, 9)
        );
        // re-declaration: still visible before the next parse
        p.add_argument(seed()).unwrap();
        p.add_argument(level()).unwrap();
        assert_eq!(p.get_int("seed").unwrap(), 77);
        assert!(p.has("level"));
        let mut a = args(&["p"]);
        p.parse_argv(&mut a, ParseFlags::REMOVE_PARSED, true)
            .unwrap();
        // no default: the old entry; default: the new declaration's
        assert_eq!(p.get_int("seed").unwrap(), 77);
        assert_eq!(p.get_int("level").unwrap(), 4);
        // a re-declared argument given again replaces the old entry
        p.add_argument(seed()).unwrap();
        let mut a = args(&["p", "--seed", "3"]);
        p.parse_argv(&mut a, ParseFlags::REMOVE_PARSED, true)
            .unwrap();
        assert_eq!(p.get_int("seed").unwrap(), 3);
        // never given, re-declared: nothing
        let mut q = ArgumentParser::default();
        q.add_argument(seed()).unwrap();
        q.add_argument(seed()).unwrap();
        q.parse(&[]).unwrap();
        assert!(!q.has("seed"));
    }

    #[test]
    fn defaults() {
        let mut p = ArgumentParser::default();
        p.add_argument(
            ArgumentSpec::new("--aka_input_file", "input").default_value("material.dat"),
        )
        .unwrap();
        p.parse(&[]).unwrap();
        assert_eq!(p.get("--aka_input_file"), Some("material.dat"));
        assert_eq!(p.get("aka_input_file"), Some("material.dat"));
        let mut p2 = p.clone();
        let mut q = ArgumentParser::default();
        q.add_argument(
            ArgumentSpec::new("--aka_input_file", "input").default_value("material.dat"),
        )
        .unwrap();
        q.parse(&args(&["--aka_input_file", "x.dat"])).unwrap();
        assert_eq!(q.get("--aka_input_file"), Some("x.dat"));
        assert_eq!(p2.get("--other"), None);
        // Re-parsing: a defaulted argument is not "parsed", so it can be
        // given once more; values accumulate (upstream appends to the
        // default), then a third occurrence is a duplicate.
        assert!(p2.parse(&args(&["--aka_input_file", "y"])).is_ok());
        assert_eq!(p2.values("aka_input_file").unwrap().len(), 2);
        assert!(p2.value("aka_input_file").is_err());
        assert!(p2.parse(&args(&["--aka_input_file", "y"])).is_err());
    }

    #[test]
    fn akantu_initialize_options() {
        let mut p = ArgumentParser::default();
        register_akantu_options(&mut p).unwrap();
        let mut argv = args(&[
            "/path/to/prog",
            "mesh.msh",
            "--aka_debug_level",
            "5",
            "--aka_print_backtrace",
            "--user_opt",
            "3",
        ]);
        let out = p
            .parse_argv(&mut argv, ParseFlags::REMOVE_PARSED, true)
            .unwrap();
        assert_eq!(out, ParseOutcome::Parsed);
        assert_eq!(
            argv,
            args(&["/path/to/prog", "mesh.msh", "--user_opt", "3"])
        );
        assert_eq!(p.get_int("aka_debug_level").unwrap(), 5);
        assert!(p.get_bool("aka_print_backtrace").unwrap());
        assert_eq!(p.get_string("aka_input_file").unwrap(), "");
        assert!(!p.has("aka_seed"));
        assert!(p.value("aka_seed").is_err());
        assert!(p.usage_string().starts_with(
            "Usage: prog [--aka_debug_level AKA_DEBUG_LEVEL] [--aka_input_file AKA_INPUT_FILE] [--aka_print_backtrace] [--aka_seed AKA_SEED] [-h]"
        ));
        let help = p.help_string();
        assert!(help.contains("  -h, --help       show this help message and exit (default: false) (const: true)\n"), "{help}");
        assert!(help.contains("(default: 4)"));
    }

    #[test]
    fn errors_and_nargs() {
        let mut p = ArgumentParser::default();
        p.add_argument(ArgumentSpec::new("-n;--number", "n").arg_type(ArgumentType::Integer))
            .unwrap();
        p.add_argument(ArgumentSpec::new("files", "f").nargs(Nargs::AtLeastOne))
            .unwrap();
        p.add_argument(
            ArgumentSpec::new("--opt", "o")
                .nargs(Nargs::OneIfPossible)
                .const_value("c"),
        )
        .unwrap();
        assert!(p.clone().parse(&args(&["-n", "x", "a"])).is_err()); // wrong type
        assert!(p.clone().parse(&args(&["-n"])).is_err()); // missing value
        assert!(p.clone().parse(&args(&["-n", "3"])).is_err()); // positional required
        let mut q = p.clone();
        q.parse(&args(&["a", "b", "--opt", "-n", "-4"])).unwrap();
        assert_eq!(q.get_int("number").unwrap(), -4);
        assert_eq!(q.get_string("opt").unwrap(), "c");
        assert_eq!(
            q.values("files").unwrap(),
            &[ArgValue::String("a".into()), ArgValue::String("b".into())]
        );
        assert!(q.value("files").is_err());
        // A greedy positional swallows unknown options...
        let mut r = p.clone();
        r.parse(&args(&["a", "--unknown"])).unwrap();
        assert_eq!(r.values("files").unwrap().len(), 2);
        // ... otherwise they are errors with `_stop_on_not_parsed`.
        assert!(
            ArgumentParser::default()
                .parse(&args(&["--unknown"]))
                .is_err()
        );
        let mut argv = args(&["prog", "--unknown"]);
        ArgumentParser::default()
            .parse_argv(&mut argv, ParseFlags::NO_FLAGS, true)
            .unwrap();
        let mut h = p.clone();
        let mut argv = args(&["prog", "-h"]);
        assert_eq!(
            h.parse_argv(&mut argv, ParseFlags::NO_FLAGS, true)
                .unwrap_err()
                .to_string(),
            "invalid argument: Argument files required but not given!"
        );
        let mut h = ArgumentParser::default();
        let mut argv = args(&["prog", "--help"]);
        assert_eq!(
            h.parse_argv(&mut argv, ParseFlags::default(), true)
                .unwrap(),
            ParseOutcome::HelpRequested
        );
        assert!(
            ArgumentParser::default()
                .add_argument(ArgumentSpec::new("-a;-b", "bad"))
                .is_err()
        );
        assert!(
            ArgumentParser::default()
                .add_argument(ArgumentSpec::new("", "bad"))
                .is_err()
        );
        assert_eq!(ArgumentSpec::new("x", "").kind(), ArgumentKind::Positional);
        assert_eq!(
            ArgumentSpec::new("--x", "").nargs(Nargs::Exactly(0)).kind(),
            ArgumentKind::Flag
        );
    }

    #[test]
    fn typed_values() {
        assert_eq!(
            convert_value(ArgumentType::Float, "1.5e3x"),
            ArgValue::Float(1500.0)
        );
        assert_eq!(
            convert_value(ArgumentType::Float, "-.5"),
            ArgValue::Float(-0.5)
        );
        assert!(check_type(ArgumentType::Float, "1e-400"));
        assert!(!check_type(ArgumentType::Float, "1e400"));
        assert!(!check_type(ArgumentType::Float, "1e"));
        assert!(!check_type(ArgumentType::Float, "abc"));
        assert!(check_type(ArgumentType::Boolean, "1"));
        assert!(!check_type(ArgumentType::Boolean, "2"));
        assert_eq!(
            convert_value(ArgumentType::Boolean, "2"),
            ArgValue::Boolean(true)
        );
        assert!(check_type(ArgumentType::Integer, "12"));
        assert!(!check_type(ArgumentType::Integer, "x"));
        assert!(!check_type(ArgumentType::Integer, "99999999999999999999"));
        assert!(!check_type(ArgumentType::String, " "));
    }
}
