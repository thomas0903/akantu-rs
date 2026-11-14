// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests (CONT-018). Behaviour of
// Akantu (EPFL/LSMS) src/model/contact_mechanics/contact_mechanics_model.cc
// `createNodalFieldReal` / `createNodalFieldInt` / `createNodalFieldBool`
// (lines 560-620): the dump field names are bound to the model's nodal
// arrays, `contact_state` is dumped through its enum → integer conversion,
// real vector fields are padded to 3 components in 2-D.
//! The resolved dump buffers of the contact model, checked by value.

mod common;

use akantu_contact::{ContactMechanicsModel, ContactState, contact_state_code};
use akantu_core::{ElementKind, GhostType, Real};
use akantu_io::dumper::{
    DumpBuffer, Dumper, FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType,
    Padding, TextDumper, pad_to_3d,
};
use akantu_mesh::Mesh;
use akantu_solver::DofManager;
use common::*;

const REAL_FIELDS: [&str; 8] = [
    "contact_force",
    "normal_force",
    "tangential_force",
    "normals",
    "tangents",
    "gaps",
    "areas",
    "tangential_traction",
];
/// Fields with `dim` components in 2-D (padded to 3 by the dumper).
const VECTOR_FIELDS: [&str; 5] = [
    "contact_force",
    "normal_force",
    "tangential_force",
    "normals",
    "tangents",
];

fn descriptor(name: &str, value_type: FieldValueType, padding: Padding) -> FieldDescriptor {
    FieldDescriptor {
        name: name.into(),
        source: FieldSource::ModelNodal(name.into()),
        support: FieldSupport::Nodal,
        value_type,
        group: None,
        padding,
        homogenize: false,
    }
}

fn resolve(
    model: &ContactMechanicsModel,
    mesh: &Mesh,
    name: &str,
    vt: FieldValueType,
) -> DumpBuffer {
    model
        .resolve(
            mesh,
            &descriptor(name, vt, Padding::None),
            None,
            GhostType::NotGhost,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn real(buffer: DumpBuffer) -> (usize, Vec<Real>) {
    match buffer {
        DumpBuffer::Real {
            nb_component,
            values,
        } => (nb_component, values),
        other => panic!("not a real buffer: {other:?}"),
    }
}

/// Distinct value of component `c` of node `n` of field number `k`.
fn marker(k: usize, n: usize, c: usize) -> Real {
    (100 * (k + 1) + 10 * n + c) as Real + 0.5
}

fn fill(a: &mut akantu_core::Array<Real>, k: usize) {
    let nbc = a.nb_component();
    for n in 0..a.len() {
        for c in 0..nbc {
            a.tuple_mut(n)[c] = marker(k, n, c);
        }
    }
}

/// [`fill`] for an exportable field (through its guard).
fn fill_shared(a: &mut akantu_core::SharedArray<Real>, k: usize) {
    let mut a = a.borrow_mut();
    let nbc = a.nb_component();
    for n in 0..a.len() {
        for c in 0..nbc {
            a.tuple_mut(n)[c] = marker(k, n, c);
        }
    }
}

/// Every name resolves to *its own* array: each model array is filled with
/// values that identify the field (`marker`), the resolved buffers must
/// carry exactly those values (a swapped mapping, e.g. `normal_force` ↔
/// `tangential_force`, cannot pass).
#[test]
fn every_dump_name_resolves_to_its_own_array_by_value() {
    let mesh = two_blocks_2d(2, 1.0, 1, 0.5);
    let dat = contact_dat("master", "slave", 0.3, 1e3, 1e2, false);
    let (shared, mut model) = contact_model(mesh, &dat);
    {
        let f = &mut model.fields;
        fill_shared(&mut f.internal_force, 0); // contact_force
        fill_shared(&mut f.normal_force, 1);
        fill_shared(&mut f.tangential_force, 2);
        fill_shared(&mut f.normals, 3);
        fill(&mut f.tangents, 4);
        fill_shared(&mut f.gaps, 5);
        fill_shared(&mut f.nodal_area, 6); // areas
        fill(&mut f.tangential_tractions, 7); // tangential_traction
    }
    let m = shared.read().unwrap();
    let nb_nodes = m.nb_nodes();
    // (dump name, field number, components in 2-D)
    for (k, (name, nbc)) in [
        ("contact_force", 2),
        ("normal_force", 2),
        ("tangential_force", 2),
        ("normals", 2),
        ("tangents", 2),
        ("gaps", 1),
        ("areas", 1),
        ("tangential_traction", 1),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(name, REAL_FIELDS[k]);
        let (c, values) = real(resolve(&model, &m, name, FieldValueType::Real));
        assert_eq!(c, nbc, "{name}: components");
        assert_eq!(values.len(), nb_nodes * nbc, "{name}: length");
        for n in 0..nb_nodes {
            for comp in 0..nbc {
                assert_eq!(
                    values[n * nbc + comp].to_bits(),
                    marker(k, n, comp).to_bits(),
                    "{name}: node {n} component {comp}"
                );
            }
        }
    }
}

/// `contact_state` is dumped through the enum → integer conversion
/// (`_no_contact` 0, `_stick` 1, `_slip` 2); `blocked_dofs` as booleans.
#[test]
fn contact_state_codes_and_blocked_dofs() {
    let mesh = two_blocks_2d(2, 1.0, 1, 0.5);
    let dat = contact_dat("master", "slave", 0.3, 1e3, 1e2, false);
    let (shared, mut model) = contact_model(mesh, &dat);
    let states = [
        ContactState::NoContact,
        ContactState::Stick,
        ContactState::Slip,
    ];
    let nb_nodes = shared.read().unwrap().nb_nodes();
    for n in 0..nb_nodes {
        model.fields.contact_state.tuple_mut(n)[0] = states[n % 3];
        model.fields.blocked_dofs.tuple_mut(n)[0] = n % 4 == 1;
    }
    let m = shared.read().unwrap();
    match resolve(&model, &m, "contact_state", FieldValueType::Int) {
        DumpBuffer::Int {
            nb_component,
            values,
        } => {
            assert_eq!(nb_component, 1);
            assert_eq!(values.len(), nb_nodes);
            for (n, v) in values.iter().enumerate() {
                assert_eq!(*v, (n % 3) as i32, "node {n}");
                assert_eq!(*v, contact_state_code(states[n % 3]));
            }
        }
        other => panic!("{other:?}"),
    }
    match resolve(&model, &m, "blocked_dofs", FieldValueType::Bool) {
        DumpBuffer::Bool {
            nb_component,
            values,
        } => {
            assert_eq!(nb_component, 1);
            let want: Vec<bool> = (0..nb_nodes).map(|n| n % 4 == 1).collect();
            assert_eq!(values, want);
        }
        other => panic!("{other:?}"),
    }
}

/// After a search and the assembly of the residual, the fields hold the
/// hand-computed values of the 2-D penalty case: the upper body (width 1,
/// slave facet length 1 → nodal area ½ at both of its interface nodes) is
/// pushed 0.01 into the lower block, `ε_n = 1e3`: gap 0.01, normal `(0, 1)`,
/// tangent `(1, 0)`, `f_N = ε_n g A n = (0, 5)`, no friction (`μ = 0`).
#[test]
fn search_and_residual_fields_have_the_hand_computed_values() {
    let mesh = two_blocks_2d(2, 1.0, 1, 0.5);
    let dat = contact_dat("master", "slave", 0.0, 1e3, 0.0, false);
    let (shared, mut model) = contact_model(mesh, &dat);
    translate_group(&mut model, &shared, "upper_body", &[0.0, -0.01]);
    model.search().unwrap();
    let mut dofs = DofManager::new("dm", akantu_comm::Communicator::serial());
    dofs.register_nodal_dofs(&shared.read().unwrap(), "displacement", 2)
        .unwrap();
    {
        use akantu_solver::SolverCallback;
        model.assemble_residual(&mut dofs).unwrap();
    }
    let m = shared.read().unwrap();
    let slaves = nodes_of_physical(&m, "slave");
    assert_eq!(slaves.len(), 2);
    let is_slave = |n: usize| slaves.contains(&(n as akantu_core::Idx));
    let (eps_n, gap, area) = (1.0e3_f64, 0.01_f64, 0.5_f64);
    let expect = |name: &str, slave: &[Real], other: &[Real]| {
        let (nbc, values) = real(resolve(&model, &m, name, FieldValueType::Real));
        for n in 0..m.nb_nodes() {
            let want = if is_slave(n) { slave } else { other };
            let got = &values[n * nbc..(n + 1) * nbc];
            for (c, (g, w)) in got.iter().zip(want).enumerate() {
                assert!(
                    (g - w).abs() <= 1e-12 * w.abs().max(1.0),
                    "{name}: node {n} component {c}: {g} vs {w}"
                );
            }
        }
    };
    let f_n = eps_n * gap * area; // 5.0
    expect("normal_force", &[0.0, f_n], &[0.0, 0.0]);
    expect("contact_force", &[0.0, f_n], &[0.0, 0.0]);
    expect("tangential_force", &[0.0, 0.0], &[0.0, 0.0]);
    expect("normals", &[0.0, 1.0], &[0.0, 0.0]);
    expect("tangents", &[1.0, 0.0], &[0.0, 0.0]);
    expect("gaps", &[gap], &[0.0]);
    expect("tangential_traction", &[0.0], &[0.0]);
    // nodal areas of the slave nodes (the master nodes' areas are not
    // part of this hand computation)
    let (_, areas) = real(resolve(&model, &m, "areas", FieldValueType::Real));
    for (n, a) in areas.iter().enumerate() {
        if is_slave(n) {
            assert!((a - area).abs() < 1e-14, "areas: node {n}: {a}");
        }
    }
}

/// Real vector fields are padded to three components in 2-D (upstream
/// `createNodalField(array, group, 3)`): the first two components are the
/// field, the third is zero; through `pad_to_3d` and end to end through the
/// dumper with `Padding::To3D`.
#[test]
fn vector_fields_are_padded_to_three_components_in_2d() {
    let mesh = two_blocks_2d(2, 1.0, 1, 0.5);
    let dat = contact_dat("master", "slave", 0.3, 1e3, 1e2, false);
    let (shared, mut model) = contact_model(mesh, &dat);
    {
        let f = &mut model.fields;
        fill_shared(&mut f.internal_force, 0);
        fill_shared(&mut f.normal_force, 1);
        fill_shared(&mut f.tangential_force, 2);
        fill_shared(&mut f.normals, 3);
        fill(&mut f.tangents, 4);
    }
    let m = shared.read().unwrap();
    let nb_nodes = m.nb_nodes();
    for (k, name) in VECTOR_FIELDS.iter().enumerate() {
        let (nbc, values) = real(resolve(&model, &m, name, FieldValueType::Real));
        assert_eq!(nbc, 2);
        let (pc, padded) = pad_to_3d(&values, nbc, 2);
        assert_eq!(pc, 3, "{name}");
        assert_eq!(padded.len(), nb_nodes * 3);
        for n in 0..nb_nodes {
            assert_eq!(padded[3 * n].to_bits(), marker(k, n, 0).to_bits(), "{name}");
            assert_eq!(
                padded[3 * n + 1].to_bits(),
                marker(k, n, 1).to_bits(),
                "{name}"
            );
            assert_eq!(padded[3 * n + 2], 0.0, "{name}: padding of node {n}");
        }
    }
    // end to end through the debug dumper with `Padding::To3D`
    let mut dumper = TextDumper::default();
    dumper
        .register_mesh(2, GhostType::NotGhost, ElementKind::Regular)
        .unwrap();
    dumper
        .register_field(descriptor(
            "normal_force",
            FieldValueType::Real,
            Padding::To3D,
        ))
        .unwrap();
    dumper.dump(&m, &model, None, Some(0)).unwrap();
    let line = dumper
        .output
        .lines()
        .find(|l| l.starts_with("normal_force"))
        .unwrap();
    let tuples: Vec<&str> = line.split(": ").nth(1).unwrap().split(" | ").collect();
    assert_eq!(tuples.len(), nb_nodes);
    for (n, t) in tuples.iter().enumerate() {
        let comps: Vec<Real> = t.split(' ').map(|x| x.parse().unwrap()).collect();
        assert_eq!(comps, [marker(1, n, 0), marker(1, n, 1), 0.0], "node {n}");
    }
}
