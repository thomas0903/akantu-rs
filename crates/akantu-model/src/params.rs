// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/parameter_registry.hh,
// parameter_registry_tmpl.hh (registerParam with runtime names),
// python/py_parser.cc:42-160 (registerParamReal from Python; UD-022: raw
// `new Real` storage in a global map, discarded random parameters).

//! Runtime-registered parameters (D-016 revised).
//!
//! Built-in laws keep their static `ParamDescriptor` tables (fast path,
//! typed members). Laws defined at run time — Python subclasses of
//! `aka.Material`, user Rust laws registered by name — store their
//! parameters in a [`DynamicParameters`] **owned by the law** (not a global
//! map, UD-022), with the same names, kinds and access flags, so `.dat`
//! files, `getParam/setParam`, `get/set` and reflection treat both alike.
//!
//! Semantics (upstream `ParameterRegistry`):
//! * names are unique; registration order is the listing order;
//! * `get` needs `READABLE`, `set` needs `WRITABLE` (not inside
//!   [`akantu_parser::param::with_upstream_write_access`], the UD-497 compat
//!   switch), `set_from_input` needs `PARSABLE`;
//! * values are converted to the declared kind by the same rule as the
//!   static tables ([`akantu_parser::param::coerce_to_kind`]): integers are
//!   accepted for real parameters (upstream's implicit conversion) and, in
//!   range, for unsigned ones; text for the byte kinds; UTF-8 bytes for the
//!   text kinds; anything else is an error;
//! * a parameter registered **without default** has no value until set;
//!   reading it is an error (UD-022 forbids reading uninitialised storage).
//!
//! **Strings.** Parameters that are an upstream
//! `std::string`/`ID` (e.g. the law's `name`) are registered as
//! [`ParamKind::ByteString`], string sets as [`ParamKind::ByteStringSet`]:
//! values from input files keep every byte (trailing white space included,
//! as `ParameterTyped<std::string>::setAuto`) and never fail; Python `str`
//! values convert losslessly. The text kinds (`String`, `Enum`, …) remain
//! for parameters that are text by design.
//!
//! **Names** are Rust text, as for the static tables. Upstream compares
//! names byte-wise; here an input-file key is decoded and compared as text
//! ([`akantu_parser::Parsable::parse_param`]), which is the same comparison
//! because every registered name is valid UTF-8 (`&str`): a UTF-8 key
//! matches exactly the name with the same bytes, and a key that is not
//! UTF-8 cannot equal any registered name, so it is unknown — as upstream,
//! whose runtime names come from Python `str` (UTF-8 encoded by the
//! bindings) or from ASCII literals.

use std::collections::HashMap;

use akantu_core::{Error, Result};
use akantu_parser::param::{ParamInfo, ParamKind, ParamValue, ParameterAccess, coerce_to_kind};

/// One runtime parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicParameter {
    /// Name.
    pub name: String,
    /// Declared kind.
    pub kind: ParamKind,
    /// Access flags.
    pub access: ParameterAccess,
    /// Description.
    pub description: String,
    value: Option<ParamValue>,
}

/// Law-owned storage of runtime-registered parameters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DynamicParameters {
    params: Vec<DynamicParameter>,
    index: HashMap<String, usize>,
}

impl DynamicParameters {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `name` (upstream `registerParam`; Python
    /// `registerParamReal(name, access, description)` or with a default).
    pub fn register(
        &mut self,
        name: &str,
        kind: ParamKind,
        access: ParameterAccess,
        description: &str,
        default: Option<ParamValue>,
    ) -> Result<()> {
        if self.index.contains_key(name) {
            return Err(Error::AlreadyExists {
                kind: "parameter",
                id: name.into(),
            });
        }
        let value = match default {
            Some(v) => Some(coerce_to_kind(name, kind, v)?),
            None => None,
        };
        self.index.insert(name.into(), self.params.len());
        self.params.push(DynamicParameter {
            name: name.into(),
            kind,
            access,
            description: description.into(),
            value,
        });
        Ok(())
    }

    fn find(&self, name: &str) -> Result<&DynamicParameter> {
        self.index
            .get(name)
            .map(|&i| &self.params[i])
            .ok_or_else(|| Error::not_found("parameter", name))
    }

    fn find_mut(&mut self, name: &str) -> Result<&mut DynamicParameter> {
        match self.index.get(name) {
            Some(&i) => Ok(&mut self.params[i]),
            None => Err(Error::not_found("parameter", name)),
        }
    }

    /// `true` if `name` is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    /// Read (`READABLE`).
    pub fn get(&self, name: &str) -> Result<ParamValue> {
        let p = self.find(name)?;
        if !p.access.contains(ParameterAccess::READABLE) {
            return Err(Error::invalid(format!(
                "parameter '{name}' is not readable"
            )));
        }
        p.value
            .clone()
            .ok_or_else(|| Error::invalid(format!("parameter '{name}' has no value yet")))
    }

    /// Real value shortcut (Python `getReal`).
    pub fn get_real(&self, name: &str) -> Result<f64> {
        self.get(name)?.as_real()
    }

    /// Write (`WRITABLE`, Python `set`; no access check inside
    /// [`akantu_parser::param::with_upstream_write_access`], UD-497).
    pub fn set(&mut self, name: &str, v: ParamValue) -> Result<()> {
        let p = self.find_mut(name)?;
        if !p.access.contains(ParameterAccess::WRITABLE)
            && !akantu_parser::param::upstream_write_access()
        {
            return Err(Error::invalid(format!(
                "parameter '{name}' is not writable"
            )));
        }
        p.value = Some(coerce_to_kind(name, p.kind, v)?);
        Ok(())
    }

    /// Write from an input file (`PARSABLE`, upstream `setAuto`).
    pub fn set_from_input(&mut self, name: &str, v: ParamValue) -> Result<()> {
        let p = self.find_mut(name)?;
        if !p.access.contains(ParameterAccess::PARSABLE) {
            return Err(Error::invalid(format!(
                "parameter '{name}' is not parsable"
            )));
        }
        p.value = Some(coerce_to_kind(name, p.kind, v)?);
        Ok(())
    }

    /// Kind and access of `name` (implements
    /// [`akantu_parser::ParameterRegistry::param_info`] for laws holding
    /// runtime parameters, so [`akantu_parser::Parsable::parse_section`]
    /// fills them from input files).
    #[must_use]
    pub fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.find(name).ok().map(|p| ParamInfo {
            kind: p.kind,
            access: p.access,
        })
    }

    /// Names in registration order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.params.iter().map(|p| p.name.clone()).collect()
    }

    /// All parameters in registration order.
    pub fn iter(&self) -> std::slice::Iter<'_, DynamicParameter> {
        self.params.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_kinds_and_defaults() {
        let mut p = DynamicParameters::new();
        let rp = ParameterAccess::READABLE | ParameterAccess::PARSMOD;
        p.register("E", ParamKind::Real, rp, "Young's modulus", None)
            .unwrap();
        p.register(
            "nu",
            ParamKind::Real,
            rp,
            "Poisson ratio",
            Some(ParamValue::Int(0)),
        )
        .unwrap();
        p.register(
            "hidden",
            ParamKind::Real,
            ParameterAccess::PARSABLE,
            "",
            None,
        )
        .unwrap();
        assert!(p.register("E", ParamKind::Real, rp, "", None).is_err());
        assert!(p.get("E").is_err(), "UD-022: no value before it is set");
        assert_eq!(p.get_real("nu").unwrap(), 0.0);
        p.set_from_input("E", ParamValue::Int(2)).unwrap();
        assert_eq!(p.get("E").unwrap(), ParamValue::Real(2.0));
        assert!(p.set("E", ParamValue::Bool(true)).is_err(), "kind checked");
        p.set_from_input("hidden", ParamValue::Real(1.0)).unwrap();
        assert!(p.get("hidden").is_err(), "not readable");
        assert!(
            p.set("hidden", ParamValue::Real(1.0)).is_err(),
            "not writable"
        );
        assert_eq!(p.names(), ["E", "nu", "hidden"]);
        assert_eq!(
            p.param_info("nu"),
            Some(ParamInfo {
                kind: ParamKind::Real,
                access: rp
            })
        );
        assert_eq!(p.param_info("missing"), None);
    }

    /// UD-497: runtime parameters follow the same
    /// write-access rule as the static tables — refused by default, assigned
    /// inside the upstream compat switch (kind still checked).
    #[test]
    fn upstream_write_access_switch() {
        let mut p = DynamicParameters::new();
        p.register(
            "lambda",
            ParamKind::Real,
            ParameterAccess::READABLE,
            "derived",
            Some(ParamValue::Real(1.0)),
        )
        .unwrap();
        assert!(p.set("lambda", ParamValue::Real(5.0)).is_err());
        assert_eq!(p.get_real("lambda").unwrap(), 1.0);
        akantu_parser::param::with_upstream_write_access(|| {
            p.set("lambda", ParamValue::Real(5.0)).unwrap();
            assert!(p.set("lambda", ParamValue::Bool(true)).is_err());
        });
        assert_eq!(p.get_real("lambda").unwrap(), 5.0);
        assert!(p.set("lambda", ParamValue::Real(6.0)).is_err());
    }

    /// Runtime string parameters use the byte kinds and the
    /// static tables' conversion rule.
    #[test]
    fn byte_kinds_keep_every_byte() {
        use std::collections::BTreeSet;

        use akantu_parser::ByteString;

        let mut p = DynamicParameters::new();
        let pr = ParameterAccess::PARSABLE | ParameterAccess::READABLE;
        p.register(
            "name",
            ParamKind::ByteString,
            pr,
            "std::string",
            Some(ParamValue::String("law".into())), // text → bytes
        )
        .unwrap();
        assert_eq!(
            p.get("name").unwrap(),
            ParamValue::ByteString(ByteString::from("law"))
        );
        // Raw input-file bytes (not UTF-8, trailing white space): unchanged.
        let raw = ByteString::from(&b"abc\xff trailing "[..]);
        p.set_from_input("name", ParamValue::ByteString(raw.clone()))
            .unwrap();
        assert_eq!(p.get("name").unwrap(), ParamValue::ByteString(raw.clone()));

        p.register(
            "surfaces",
            ParamKind::ByteStringSet,
            ParameterAccess::PARSMOD,
            "std::set<ID>",
            None,
        )
        .unwrap();
        let text: BTreeSet<String> = ["b".to_owned(), "a".to_owned()].into();
        p.set("surfaces", ParamValue::StringSet(text)).unwrap();
        let bytes: BTreeSet<ByteString> = ["a", "b"].into_iter().map(ByteString::from).collect();
        assert_eq!(p.get("surfaces").unwrap(), ParamValue::ByteStringSet(bytes));

        // Rust text kinds: UTF-8 bytes are accepted, other bytes rejected.
        p.register(
            "label",
            ParamKind::String,
            ParameterAccess::PARSMOD,
            "",
            None,
        )
        .unwrap();
        p.set_from_input("label", ParamValue::ByteString("ok".into()))
            .unwrap();
        assert_eq!(p.get("label").unwrap(), ParamValue::String("ok".into()));
        assert!(
            p.set_from_input("label", ParamValue::ByteString(raw))
                .is_err()
        );
        assert_eq!(p.get("label").unwrap(), ParamValue::String("ok".into()));

        // Unsigned parameters accept in-range integers (as the tables).
        p.register("n", ParamKind::UInt, ParameterAccess::PARSMOD, "", None)
            .unwrap();
        p.set("n", ParamValue::Int(7)).unwrap();
        assert_eq!(p.get("n").unwrap(), ParamValue::Int(7));
        assert!(p.set("n", ParamValue::Int(-1)).is_err());
    }
}
