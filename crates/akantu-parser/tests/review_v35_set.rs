// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// `vg04_*` (UD-497, D-003 category B): upstream
// src/io/parser/parameter_registry_tmpl.hh:154-160 (`ParameterTyped<T>::set`
// assigns without an access check) and parameter_registry.cc:137-152
// (`ParameterRegistry::set`: own parameter, else every sub-registry, then
// `updateInternalParameters`), against the documented meaning of
// `_pat_writable` (doc/dev-doc/manual/new-constitutive-laws.rst:178).

//! `review_v35_set` (row PARSE-008): by default a registry refuses to write
//! a parameter without `WRITABLE`, and a refused recursive `set` leaves
//! **no** partial update (the access of every registry the recursion would
//! write is checked before anything is written or updated). Inside
//! [`with_upstream_write_access`] (the UD-497 compat switch) the write
//! reproduces upstream: assign without access check, then
//! `updateInternalParameters`, sub-registries before their parent.

use std::cell::RefCell;

use akantu_core::Result;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    SubRegistryDescriptor, table_get, table_info, table_names, table_set, table_sub_registry,
    table_sub_registry_ids, table_sub_registry_mut, upstream_write_access,
    with_upstream_write_access,
};

thread_local! {
    static EVENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn take_events() -> Vec<String> {
    EVENTS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

#[derive(Debug)]
struct Leaf {
    name: &'static str,
    shared: f64,
}

static WRITABLE_LEAF: &[ParamDescriptor<Leaf>] = &[ParamDescriptor::new(
    "shared",
    ParamKind::Real,
    ParameterAccess::PARSMOD,
    "writable",
    |s| ParamValue::Real(s.shared),
    Some(|s, v| {
        s.shared = v.as_real()?;
        Ok(())
    }),
)];

static READ_ONLY_LEAF: &[ParamDescriptor<Leaf>] = &[ParamDescriptor::new(
    "shared",
    ParamKind::Real,
    ParameterAccess::READABLE,
    "read only (with a setter, reachable by the compat switch)",
    |s| ParamValue::Real(s.shared),
    Some(|s, v| {
        s.shared = v.as_real()?;
        Ok(())
    }),
)];

#[derive(Debug)]
struct Writable(Leaf);
#[derive(Debug)]
struct ReadOnly(Leaf);

macro_rules! leaf_registry {
    ($ty:ident, $table:ident) => {
        impl ParameterRegistry for $ty {
            fn get_param(&self, name: &str) -> Result<ParamValue> {
                table_get($table, &self.0, name)
            }
            fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
                table_set($table, &mut self.0, name, value)
            }
            fn param_names(&self) -> Vec<&'static str> {
                table_names($table)
            }
            fn param_info(&self, name: &str) -> Option<ParamInfo> {
                table_info($table, name)
            }
            fn update_internal_parameters(&mut self) -> Result<()> {
                EVENTS.with(|v| v.borrow_mut().push(format!("update:{}", self.0.name)));
                Ok(())
            }
            fn considers_sub_registries(&self) -> bool {
                true
            }
        }
    };
}
leaf_registry!(Writable, WRITABLE_LEAF);
leaf_registry!(ReadOnly, READ_ONLY_LEAF);

/// Root `R` with sub-registries `a` (writable `shared`) and `b` (read-only
/// `shared`): upstream sets `a` first (id order), then fails or not at `b`.
#[derive(Debug)]
struct Root {
    a: Writable,
    b: ReadOnly,
}

static ROOT_SUBS: &[SubRegistryDescriptor<Root>] = &[
    SubRegistryDescriptor::new("b", |s| &s.b, |s| &mut s.b),
    SubRegistryDescriptor::new("a", |s| &s.a, |s| &mut s.a),
];

impl ParameterRegistry for Root {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        table_get::<Root>(&[], self, name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set::<Root>(&[], self, name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        EVENTS.with(|v| v.borrow_mut().push("update:R".to_owned()));
        Ok(())
    }
    fn sub_registry_ids(&self) -> Vec<&str> {
        table_sub_registry_ids(ROOT_SUBS, &[])
    }
    fn sub_registry(&self, id: &str) -> Option<&dyn ParameterRegistry> {
        table_sub_registry(ROOT_SUBS, &[], self, id)
    }
    fn sub_registry_mut(&mut self, id: &str) -> Option<&mut dyn ParameterRegistry> {
        table_sub_registry_mut(ROOT_SUBS, &[], self, id)
    }
    fn considers_sub_registries(&self) -> bool {
        true
    }
}

fn root() -> Root {
    Root {
        a: Writable(Leaf {
            name: "A",
            shared: 1.0,
        }),
        b: ReadOnly(Leaf {
            name: "B",
            shared: 2.0,
        }),
    }
}

/// `vg04_*` default (UD-497, B): the refusal of `B.shared` happens before
/// anything is written: `A.shared` keeps its value and no
/// `updateInternalParameters` runs.
#[test]
fn vg04_refused_recursive_set_leaves_no_partial_update() {
    let mut r = root();
    take_events();
    let res = r.set_param_recursive("shared", ParamValue::Real(9.0));
    assert!(
        res.as_ref()
            .is_err_and(|e| e.to_string().contains("not writable")),
        "{res:?}"
    );
    assert_eq!((r.a.0.shared, r.b.0.shared), (1.0, 2.0));
    assert_eq!(take_events(), Vec::<String>::new(), "partial update");
    // a direct write of a read-only parameter is refused too
    assert!(r.b.set_param("shared", ParamValue::Real(9.0)).is_err());
    assert_eq!(r.b.0.shared, 2.0);
    // writable parameters are unaffected
    r.a.set_param_recursive("shared", ParamValue::Real(4.0))
        .unwrap();
    assert_eq!(r.a.0.shared, 4.0);
    assert_eq!(take_events(), ["update:A"]);
}

/// `vg04_*` compat switch: upstream assigns without access check and updates
/// every registry reached (sub-registries before the parent); the switch
/// is scoped to the closure.
#[test]
fn vg04_upstream_write_access_reproduces_upstream() {
    let mut r = root();
    take_events();
    assert!(!upstream_write_access());
    let res = with_upstream_write_access(|| {
        assert!(upstream_write_access());
        r.set_param_recursive("shared", ParamValue::Real(9.0))
    });
    res.unwrap();
    assert!(!upstream_write_access(), "switch not restored");
    assert_eq!((r.a.0.shared, r.b.0.shared), (9.0, 9.0));
    assert_eq!(take_events(), ["update:A", "update:B", "update:R"]);
    // nested scopes restore the outer state
    with_upstream_write_access(|| {
        with_upstream_write_access(|| ());
        assert!(upstream_write_access());
    });
    assert!(!upstream_write_access());
    // other threads are not affected
    with_upstream_write_access(|| {
        let other = std::thread::spawn(upstream_write_access).join().unwrap();
        assert!(!other);
    });
}
