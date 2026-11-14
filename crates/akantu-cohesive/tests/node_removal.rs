// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests
// (SMM-015, UD-209): node removal under a live cohesive model after
// insertion (upstream solid_mechanics_model.cc:752-783 onNodesRemoved,
// dof_manager.cc:901-902, mesh_utils.cc purifyMesh); the scenario is
// upstream's mode-I extrinsic fixture (test_cohesive_fixture.hh: initial
// strain 1.1 σc/E opens the interface, then StrainIncrement steps).

//! The removal is a renumbering of the doubled nodes: `purify` after the
//! insertion moves every node to its first appearance in the connectivity
//! (the doubled nodes, appended at the end by the insertion, move between
//! the original ones). Criterion: the run with the removal equals a **twin
//! run without it**, node by node through the numbering, bitwise — explicit
//! central differences with a lumped mass make every nodal value
//! independent of the equation numbering (nodal sums are accumulated in
//! element order in both runs), so the comparison isolates what the
//! removal must preserve: every nodal array (DOF family and model arrays),
//! the masses (reassembled after the removal, kept in the twin) and the
//! cohesive laws' state (element-indexed, untouched). A fresh cohesive
//! model on the renumbered mesh would need the whole cohesive state
//! (inserter, facet stresses, every law internal) transferred; the twin
//! carries it by construction.

mod common;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_mesh::{MeshEvent, MeshPart};
use common::{Fixture, MATERIAL_0};

const MESH: &str = "_cohesive_2d_4_triangle_3.msh";
const COH: ElementType = ElementType::Cohesive2d4;

/// The mode-I run: insertion of the whole interface by the stress check,
/// `before` steps, the removal (when `remove`), `after` steps. Returns the
/// model and the numbering of the removal (identity without it).
fn run(remove: bool, before: usize, after: usize) -> (Fixture, Vec<Idx>) {
    let mut f = Fixture::create(MESH, 2, COH, MATERIAL_0, true);
    let strain = f.mode_i_strain();
    let init: Vec<Real> = strain.iter().map(|s| s * 1.1).collect();
    f.set_initial_condition(&init);
    f.model.assemble_internal_forces().unwrap();
    let n = f.model.check_cohesive_stress().unwrap();
    assert_eq!(n, f.group_size, "the check opens the interface");
    f.nb_steps = before;
    f.steps(&strain, |_, _, _| {});
    let nb = f.mesh.read().unwrap().nb_nodes();
    let mut numbering: Vec<Idx> = (0..nb as Idx).collect();
    if remove {
        let committed = f
            .model
            .mutate_mesh(|tx| tx.purify(MeshPart::Main), None)
            .unwrap();
        let ev = committed
            .events
            .iter()
            .find_map(|(p, e)| match e {
                MeshEvent::NodesRemoved { new_numbering, .. } if *p == MeshPart::Main => {
                    Some(new_numbering.clone())
                }
                _ => None,
            })
            .expect("the renumbering is dispatched");
        numbering = ev;
    }
    f.nb_steps = after;
    f.steps(&strain, |_, _, _| {});
    (f, numbering)
}

fn nodal(m: &SolidMechanicsModelCohesive) -> Vec<(&'static str, Vec<Real>)> {
    vec![
        ("u", m.smm.displacement().unwrap().as_slice().to_vec()),
        ("v", m.smm.velocity().unwrap().as_slice().to_vec()),
        ("a", m.smm.acceleration().unwrap().as_slice().to_vec()),
        ("f_int", m.smm.internal_force().unwrap().as_slice().to_vec()),
        ("f_ext", m.smm.external_force().unwrap().as_slice().to_vec()),
    ]
}

fn law_state(m: &SolidMechanicsModelCohesive) -> Vec<(&'static str, Vec<Real>)> {
    let mat = m.cohesive_material("insertion").unwrap();
    ["opening", "tractions", "damage", "delta max"]
        .into_iter()
        .map(|n| {
            (
                n,
                mat.array(n, COH, GhostType::NotGhost)
                    .unwrap()
                    .as_slice()
                    .to_vec(),
            )
        })
        .collect()
}

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn removal_after_insertion_equals_the_twin_run() {
    let _g = common::lock();
    let (with, numbering) = run(true, 8, 12);
    let (twin, _) = run(false, 8, 12);
    let nb = numbering.len();
    assert!(
        numbering
            .iter()
            .enumerate()
            .any(|(i, &n)| usize::try_from(n).is_ok_and(|n| n != i)),
        "purify after the insertion moves nodes"
    );
    assert_eq!(with.mesh.read().unwrap().nb_nodes(), nb);
    // the numbering is the fresh registration's on the renumbered mesh
    // (serial: identity, 2 components; the twin's insertion block also
    // continues the local numbering, so both are the identity)
    let eqs = with
        .model
        .smm
        .solver
        .dof_manager
        .local_equation_numbers("displacement")
        .unwrap()
        .to_vec();
    assert_eq!(eqs, (0..2 * nb as Idx).collect::<Vec<_>>());
    for ((name, x), (_, y)) in nodal(&with.model).iter().zip(nodal(&twin.model)) {
        let mut moved = vec![0.0; y.len()];
        for (old, &new) in numbering.iter().enumerate() {
            let new = new as usize;
            moved[2 * new..2 * new + 2].copy_from_slice(&y[2 * old..2 * old + 2]);
        }
        assert_eq!(bits(x), bits(&moved), "{name}");
        assert!(
            x.iter().any(|&v| v != 0.0) || *name == "f_ext",
            "{name} vacuous"
        );
    }
    let blocked: Vec<bool> = with.model.smm.blocked_dofs().unwrap().as_slice().to_vec();
    let twin_blocked: Vec<bool> = twin.model.smm.blocked_dofs().unwrap().as_slice().to_vec();
    for (old, &new) in numbering.iter().enumerate() {
        let new = new as usize;
        assert_eq!(
            blocked[2 * new..2 * new + 2],
            twin_blocked[2 * old..2 * old + 2]
        );
    }
    for ((name, x), (_, y)) in law_state(&with.model).iter().zip(law_state(&twin.model)) {
        assert_eq!(bits(x), bits(&y), "cohesive {name}");
    }
    assert!(
        law_state(&with.model)[0].1.iter().any(|&o| o != 0.0),
        "the interface opens"
    );
}

/// The interface's checkable facets whose barycentre lies right of the
/// mesh's middle (x), and their type.
fn right_half(f: &Fixture) -> (ElementType, Vec<usize>) {
    let m = f.mesh.read().unwrap();
    let x: Vec<Real> = m.nodes().as_slice().chunks(2).map(|p| p[0]).collect();
    let mid = 0.5
        * (x.iter().copied().fold(Real::INFINITY, Real::min)
            + x.iter().copied().fold(Real::NEG_INFINITY, Real::max));
    let fm = m.mesh_facets().unwrap();
    let ty = COH.facet_type(0).unwrap();
    let check = f
        .model
        .ext
        .inserter
        .check_facets()
        .get(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    let mut b = [0.0; 2];
    let right = check
        .iter()
        .enumerate()
        .filter(|(_, c)| **c)
        .filter_map(|(k, _)| {
            let e = akantu_core::Element::new(ty, k as Idx, GhostType::NotGhost);
            fm.barycenter_into(e, &mut b).unwrap();
            (b[0] > mid).then_some(k)
        })
        .collect();
    (ty, right)
}

/// A partial insertion, the removal, a further
/// insertion. The right half of the interface is not checked at first
/// (its check flags cleared), so the stress check inserts the left half;
/// after `before` steps the removal renumbers the nodes (when `remove`),
/// then the right half is checkable again and the steps' checks insert it
/// — the inserter, the facet data and the cohesive laws work on the
/// renumbered mesh. Returns the model, the removal's numbering (identity
/// without it) and the node count before the second insertion.
fn run_partial(remove: bool, before: usize, after: usize) -> (Fixture, Vec<Idx>, usize) {
    let mut f = Fixture::create(MESH, 2, COH, MATERIAL_0, true);
    let strain = f.mode_i_strain();
    let init: Vec<Real> = strain.iter().map(|s| s * 1.1).collect();
    f.set_initial_condition(&init);
    f.model.assemble_internal_forces().unwrap();
    let (ty, right) = right_half(&f);
    let set_right = |f: &mut Fixture, v: bool| {
        let c = f.model.ext.inserter.check_facets_mut();
        let c = c.get_mut(ty, GhostType::NotGhost).unwrap().as_mut_slice();
        for &k in &right {
            c[k] = v;
        }
    };
    set_right(&mut f, false);
    let first = f.model.check_cohesive_stress().unwrap();
    assert!(
        first > 0 && first + right.len() == f.group_size,
        "the left half is inserted first ({first} + {} of {})",
        right.len(),
        f.group_size
    );
    f.nb_steps = before;
    f.steps(&strain, |_, _, n| assert_eq!(n, 0, "the right half waits"));
    let nb = f.mesh.read().unwrap().nb_nodes();
    let mut numbering: Vec<Idx> = (0..nb as Idx).collect();
    if remove {
        let committed = f
            .model
            .mutate_mesh(|tx| tx.purify(MeshPart::Main), None)
            .unwrap();
        numbering = committed
            .events
            .iter()
            .find_map(|(p, e)| match e {
                MeshEvent::NodesRemoved { new_numbering, .. } if *p == MeshPart::Main => {
                    Some(new_numbering.clone())
                }
                _ => None,
            })
            .expect("the renumbering is dispatched");
    }
    set_right(&mut f, true);
    let mut second = 0;
    f.nb_steps = after;
    f.steps(&strain, |_, _, n| second += n);
    assert_eq!(second, right.len(), "the right half is inserted afterwards");
    (f, numbering, nb)
}

#[test]
fn insertion_after_a_removal_equals_the_twin_run() {
    let _g = common::lock();
    let (with, numbering, nb) = run_partial(true, 6, 14);
    let (twin, _, nb_twin) = run_partial(false, 6, 14);
    assert_eq!(nb, nb_twin);
    assert!(
        numbering
            .iter()
            .enumerate()
            .any(|(i, &n)| usize::try_from(n).is_ok_and(|n| n != i)),
        "purify after the first insertion moves nodes"
    );
    // the twin's node `old` is the removal run's `map[old]`: renumbered up
    // to the removal, then the second insertion's nodes appended in the
    // same order in both runs (checked through their coordinates)
    let total = with.mesh.read().unwrap().nb_nodes();
    assert_eq!(total, twin.mesh.read().unwrap().nb_nodes());
    assert!(total > nb, "the second insertion doubles nodes");
    let map: Vec<usize> = (0..total)
        .map(|o| {
            if o < nb {
                usize::try_from(numbering[o]).unwrap()
            } else {
                o
            }
        })
        .collect();
    {
        let xw = with.mesh.read().unwrap().nodes().as_slice().to_vec();
        let xt = twin.mesh.read().unwrap().nodes().as_slice().to_vec();
        for (old, &new) in map.iter().enumerate() {
            assert_eq!(
                bits(&xw[2 * new..2 * new + 2]),
                bits(&xt[2 * old..2 * old + 2]),
                "node {old} -> {new}: coordinates"
            );
        }
    }
    for ((name, x), (_, y)) in nodal(&with.model).iter().zip(nodal(&twin.model)) {
        let mut moved = vec![0.0; y.len()];
        for (old, &new) in map.iter().enumerate() {
            moved[2 * new..2 * new + 2].copy_from_slice(&y[2 * old..2 * old + 2]);
        }
        assert_eq!(bits(x), bits(&moved), "{name}");
        assert!(
            x.iter().any(|&v| v != 0.0) || *name == "f_ext",
            "{name} vacuous"
        );
    }
    for ((name, x), (_, y)) in law_state(&with.model).iter().zip(law_state(&twin.model)) {
        assert_eq!(bits(x), bits(&y), "cohesive {name}");
    }
    let n = with
        .mesh
        .read()
        .unwrap()
        .nb_element(COH, GhostType::NotGhost);
    assert_eq!(n, with.group_size, "the whole interface is inserted");
}
