// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the port
// extensions `save_previous_state` / `search_keeping_history` (UD-374)
// around
// src/model/contact_mechanics/contact_mechanics_model.cc (`search`,
// `savePreviousState`).

mod common;

use akantu_contact::ContactMechanicsModel;
use akantu_core::{Element, Real};
use akantu_mesh::SharedMesh;
use common::*;

/// Frictional 2-D two-block model, upper body pressed into the lower one.
fn setup() -> (SharedMesh, ContactMechanicsModel) {
    let mesh = two_blocks_2d(8, 1.0, 2, 0.4);
    let (shared, mut m) = contact_model(mesh, &contact_dat("master", "slave", 0.3, 1e3, 1e2, true));
    translate_group(&mut m, &shared, "upper_body", &[0.031, -0.06]);
    (shared, m)
}

fn bits(a: &[Real]) -> Vec<u64> {
    a.iter().map(|v| v.to_bits()).collect()
}

/// Every field a search or a force assembly writes, as bit patterns.
fn state(m: &ContactMechanicsModel) -> Vec<(String, Vec<u64>)> {
    let f = &m.fields;
    let mut v = vec![
        ("gaps".into(), bits(f.gaps.borrow().as_slice())),
        ("normals".into(), bits(f.normals.borrow().as_slice())),
        ("tangents".into(), bits(f.tangents.as_slice())),
        ("projections".into(), bits(f.projections.as_slice())),
        ("tractions".into(), bits(f.tangential_tractions.as_slice())),
        ("areas".into(), bits(f.nodal_area.borrow().as_slice())),
        ("force".into(), bits(f.internal_force.borrow().as_slice())),
    ];
    v.extend(history(m));
    v.push((
        "contact_state".into(),
        f.contact_state
            .as_slice()
            .iter()
            .map(|s| akantu_contact::contact_state_code(*s) as u64)
            .collect(),
    ));
    v.push((
        "contact_elements".into(),
        m.contact_elements()
            .iter()
            .flat_map(|e| [e.slave as u64, e.master.index() as u64])
            .collect(),
    ));
    v
}

/// The `previous_*` history as bit patterns (master elements as indices,
/// `u64::MAX` for `Element::NULL`).
fn history(m: &ContactMechanicsModel) -> Vec<(String, Vec<u64>)> {
    let f = &m.fields;
    vec![
        (
            "previous_projections".into(),
            bits(f.previous_projections.as_slice()),
        ),
        (
            "previous_tangents".into(),
            bits(f.previous_tangents.as_slice()),
        ),
        (
            "previous_tractions".into(),
            bits(f.previous_tangential_tractions.as_slice()),
        ),
        (
            "previous_masters".into(),
            f.previous_master_elements
                .as_slice()
                .iter()
                .map(|e| {
                    if e.is_null() {
                        u64::MAX
                    } else {
                        e.index() as u64
                    }
                })
                .collect(),
        ),
    ]
}

/// `search()` is bitwise `save_previous_state()` followed by
/// `search_keeping_history()`, over a sequence of searches and force
/// assemblies with a sliding upper body (friction history non-zero).
#[test]
fn search_is_save_then_search_keeping_history() {
    let (shared, mut a) = setup();
    let (_, mut b) = setup();
    let mut with_history = 0;
    for step in 0..6 {
        a.search().unwrap();
        b.save_previous_state().unwrap();
        b.search_keeping_history().unwrap();
        assert_eq!(state(&a), state(&b), "after the search of step {step}");
        a.assemble_internal_forces().unwrap();
        b.assemble_internal_forces().unwrap();
        assert_eq!(state(&a), state(&b), "after the forces of step {step}");
        if a.fields
            .previous_tangential_tractions
            .as_slice()
            .iter()
            .any(|&t| t != 0.0)
        {
            with_history += 1;
        }
        translate_group(&mut a, &shared, "upper_body", &[0.004, -0.001]);
        translate_group(&mut b, &shared, "upper_body", &[0.004, -0.001]);
    }
    assert!(!a.contact_elements().is_empty());
    assert!(with_history >= 3, "the friction history stayed empty");
}

/// `search_keeping_history()` leaves the `previous_*` history untouched
/// while the current fields move on (`search()` would have overwritten it).
#[test]
fn search_keeping_history_leaves_the_history_untouched() {
    let (shared, mut m) = setup();
    for _ in 0..2 {
        m.search().unwrap();
        m.assemble_internal_forces().unwrap();
        translate_group(&mut m, &shared, "upper_body", &[0.004, 0.0]);
    }
    m.search().unwrap();
    m.assemble_internal_forces().unwrap();
    let committed = history(&m);
    assert!(
        committed[2].1.iter().any(|&t| f64::from_bits(t) != 0.0),
        "non-zero traction history expected"
    );
    let current_projections = bits(m.fields.projections.as_slice());
    for _ in 0..3 {
        translate_group(&mut m, &shared, "upper_body", &[0.003, -0.0005]);
        m.search_keeping_history().unwrap();
        m.assemble_internal_forces().unwrap();
        assert_eq!(history(&m), committed, "the history changed");
    }
    assert_ne!(
        bits(m.fields.projections.as_slice()),
        current_projections,
        "the current projections should have moved"
    );
}

/// History arrays shorter than the node count (a history saved before
/// nodes were added, e.g. by cohesive insertion) are extended with empty
/// rows by `search_keeping_history()`; existing rows are kept.
#[test]
fn search_keeping_history_extends_the_history_to_new_nodes() {
    let (shared, mut m) = setup();
    m.search().unwrap();
    m.assemble_internal_forces().unwrap();
    translate_group(&mut m, &shared, "upper_body", &[0.004, 0.0]);
    m.search().unwrap();
    m.assemble_internal_forces().unwrap();
    m.save_previous_state().unwrap();
    let nb = m.positions().len();
    let keep = nb - 5;
    let before = history(&m);
    {
        let f = &mut m.fields;
        f.previous_projections.try_resize(keep, 0.0).unwrap();
        f.previous_tangents.try_resize(keep, 0.0).unwrap();
        f.previous_tangential_tractions
            .try_resize(keep, 0.0)
            .unwrap();
        f.previous_master_elements
            .try_resize(keep, Element::NULL)
            .unwrap();
    }
    m.search_keeping_history().unwrap();
    let f = &m.fields;
    assert_eq!(f.previous_projections.len(), nb);
    assert_eq!(f.previous_tangents.len(), nb);
    assert_eq!(f.previous_tangential_tractions.len(), nb);
    assert_eq!(f.previous_master_elements.len(), nb);
    let after = history(&m);
    for ((name, b), (_, a)) in before.iter().zip(&after) {
        let nc = b.len() / nb;
        assert_eq!(
            &a[..keep * nc],
            &b[..keep * nc],
            "{name}: kept rows changed"
        );
        let empty = if name == "previous_masters" {
            u64::MAX
        } else {
            0
        };
        assert!(
            a[keep * nc..].iter().all(|&v| v == empty),
            "{name}: new rows not empty"
        );
    }
}
