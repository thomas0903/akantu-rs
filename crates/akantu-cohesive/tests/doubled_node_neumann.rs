// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). D-105 / UD-1280:
// independent physical check of the Neumann load of a
// doubled node, on upstream's cohesive test fixture (test_cohesive/
// test_cohesive_fixture.hh, _cohesive_2d_4_triangle_3.msh: the plate
// [-1, 1] × [-0.5, 0.5] with the interface "insertion" at y = 0 reaching the
// faces "sides" at x = ±1) and Akantu's solid_mechanics_model_cohesive.cc
// (`onNodesAdded`).

//! A constant traction `t` on "sides" (the faces the crack reaches) and a
//! nodal force `p` written by the user at the left crack tip node, then the
//! whole interface inserted through the regular stress check (mode-I
//! initial strain, as upstream's fixture). After the doubling:
//!
//! * **default** (D-105): the external force of each lip (the nodes whose
//!   triangles lie above, resp. below the crack) equals the
//!   integral of `t` over that lip's boundary segments, `t·2·(y_max − y_c)`
//!   and `t·2·(y_c − y_min)` (closed form), plus `p` on the lip of the
//!   original copy of the user's node (a nodal force of other origin stays
//!   with the original copy);
//! * **switch** `doubled_node_keeps_external_force` (upstream): it does not:
//!   each original copy keeps the load of the segments its new copy now
//!   uses, `t·Σ ℓ/2` over those segments (closed form from the mesh), which
//!   the test computes and asserts — the size of the misplacement;
//! * in both modes the sum of the external force over all copies is
//!   unchanged by the doubling, and computing the fragments leaves the
//!   record unchanged;
//! * the "zero the external force, apply the traction again" loop gives the
//!   same lips as one application (the record forgets the reset nodes).

mod common;

use akantu_cohesive::FragmentManager;
use akantu_core::{ElementType, GhostType, Real};
use akantu_model::boundary_condition::FromTraction;
use common::{Fixture, MATERIAL_0};

const MESH: &str = "_cohesive_2d_4_triangle_3.msh";
const T: [Real; 2] = [3.0e5, -2.0e5];
const P: [Real; 2] = [1.0e3, 7.0e2];
const G: GhostType = GhostType::NotGhost;

#[derive(Debug)]
struct Outcome {
    /// external force summed over the upper / lower lip
    lips: [[Real; 2]; 2],
    /// closed form of the default: `t·L` (+ `p` on the original's lip)
    expected: [[Real; 2]; 2],
    /// upstream's misplacement per lip (closed form from the mesh)
    misplaced: [[Real; 2]; 2],
    total_before: [Real; 2],
    total_after: [Real; 2],
    doubled_loaded: usize,
}

/// `lip(n)`: 0 above the crack (mean barycentre of the node's triangles),
/// 1 below.
fn lips_of(m: &akantu_mesh::Mesh, yc: Real) -> Vec<usize> {
    let x = m.nodes();
    let x = x.as_slice();
    let conn = m.connectivity(ElementType::Triangle3, G).unwrap();
    let mut sum = vec![0.0; m.nb_nodes()];
    let mut cnt = vec![0usize; m.nb_nodes()];
    for e in 0..conn.len() {
        let t = conn.tuple(e);
        let by: Real = t.iter().map(|&n| x[2 * n as usize + 1]).sum::<Real>() / 3.0;
        for &n in t {
            sum[n as usize] += by;
            cnt[n as usize] += 1;
        }
    }
    (0..m.nb_nodes())
        .map(|n| usize::from(sum[n] / cnt[n].max(1) as Real <= yc))
        .collect()
}

fn run(keep: bool, reset_loop: bool) -> Outcome {
    let mut f = Fixture::create_with(MESH, 2, ElementType::Cohesive2d4, MATERIAL_0, true, |m| {
        m.ext.compat.doubled_node_keeps_external_force = keep;
    });
    // geometry: the crack line and the user's node (left crack tip)
    let (yc, ymin, ymax, tip, n0) = {
        let m = f.mesh.read().unwrap();
        let x = m.nodes().as_slice().to_vec();
        let ins = m.groups().element_group("insertion").unwrap();
        let nodes = ins.nodes().as_slice().to_vec();
        let yc = x[2 * nodes[0] as usize + 1];
        assert!(
            nodes.iter().all(|&n| x[2 * n as usize + 1] == yc),
            "a straight crack"
        );
        let tip = *nodes
            .iter()
            .min_by(|&&a, &&b| x[2 * a as usize].total_cmp(&x[2 * b as usize]))
            .unwrap();
        (
            yc,
            m.lower_bounds()[1],
            m.upper_bounds()[1],
            tip as usize,
            m.nb_nodes(),
        )
    };
    let traction = FromTraction::new(T.to_vec());
    f.model.smm.apply_neumann_bc(&traction, "sides").unwrap();
    if reset_loop {
        for _ in 0..2 {
            f.model
                .smm
                .external_force_mut()
                .unwrap()
                .as_mut_slice()
                .fill(0.0);
            f.model.smm.apply_neumann_bc(&traction, "sides").unwrap();
        }
    }
    {
        let mut fe = f.model.smm.external_force_mut().unwrap();
        fe.as_mut_slice()[2 * tip] += P[0];
        fe.as_mut_slice()[2 * tip + 1] += P[1];
    }
    let total = |f: &Fixture| {
        let fe = f.model.smm.external_force().unwrap();
        let mut s = [0.0; 2];
        for c in fe.as_slice().chunks(2) {
            s[0] += c[0];
            s[1] += c[1];
        }
        s
    };
    let total_before = total(&f);
    // the whole interface through the regular stress check
    let strain = f.mode_i_strain();
    let init: Vec<Real> = strain.iter().map(|s| s * 1.1).collect();
    f.set_initial_condition(&init);
    f.model.assemble_internal_forces().unwrap();
    let inserted = f.model.check_cohesive_stress().unwrap();
    assert_eq!(inserted, f.group_size, "the check opens the interface");
    let total_after = total(&f);
    // computing the fragments leaves the record unchanged
    let record = f.model.smm.neumann_store().unwrap().forces().clone();
    FragmentManager::new(&f.model)
        .unwrap()
        .compute_all_data(&mut f.model, 1.0)
        .unwrap();
    assert_eq!(
        format!("{:?}", f.model.smm.neumann_store().unwrap().forces()),
        format!("{record:?}"),
        "fragments do not touch the record"
    );

    let m = f.mesh.read().unwrap();
    let x = m.nodes().as_slice().to_vec();
    let lip = lips_of(&m, yc);
    let fe = f.model.smm.external_force().unwrap().as_slice().to_vec();
    let mut lips = [[0.0; 2]; 2];
    for n in 0..m.nb_nodes() {
        for d in 0..2 {
            lips[lip[n]][d] += fe[2 * n + d];
        }
    }
    // the original copy of the user's node (index below n0)
    let mut expected = [
        [T[0] * 2.0 * (ymax - yc), T[1] * 2.0 * (ymax - yc)],
        [T[0] * 2.0 * (yc - ymin), T[1] * 2.0 * (yc - ymin)],
    ];
    for d in 0..2 {
        expected[lip[tip]][d] += P[d];
    }
    // upstream's misplacement: each new copy's own segments of "sides"
    let sides = m.groups().element_group("sides").unwrap();
    let segs = sides.elements_of(ElementType::Segment2, G).to_vec();
    let conn = m.connectivity(ElementType::Segment2, G).unwrap();
    let mut misplaced = [[0.0; 2]; 2];
    let mut doubled_loaded = 0;
    for new in n0..m.nb_nodes() {
        let old = (0..n0)
            .find(|&o| x[2 * o] == x[2 * new] && x[2 * o + 1] == x[2 * new + 1])
            .expect("the original copy");
        let mut len = 0.0;
        for &e in &segs {
            let t = conn.tuple(e as usize);
            if t.contains(&(new as akantu_core::Idx)) {
                let (a, b) = (t[0] as usize, t[1] as usize);
                len +=
                    ((x[2 * a] - x[2 * b]).powi(2) + (x[2 * a + 1] - x[2 * b + 1]).powi(2)).sqrt();
            }
        }
        if len > 0.0 {
            doubled_loaded += 1;
            for d in 0..2 {
                misplaced[lip[old]][d] += T[d] * len / 2.0;
                misplaced[lip[new]][d] -= T[d] * len / 2.0;
            }
        }
    }
    Outcome {
        lips,
        expected,
        misplaced,
        total_before,
        total_after,
        doubled_loaded,
    }
}

fn scale() -> Real {
    T[0].abs().max(T[1].abs())
}

fn assert_close(what: &str, a: [[Real; 2]; 2], b: [[Real; 2]; 2]) {
    for l in 0..2 {
        for d in 0..2 {
            assert!(
                (a[l][d] - b[l][d]).abs() <= 1e-12 * scale(),
                "{what}: lip {l} component {d}: {} vs {}",
                a[l][d],
                b[l][d]
            );
        }
    }
}

fn conserved(o: &Outcome) {
    for d in 0..2 {
        assert!(
            (o.total_before[d] - o.total_after[d]).abs() <= 1e-12 * scale(),
            "the doubling conserves the total: {:?} vs {:?}",
            o.total_before,
            o.total_after
        );
    }
}

#[test]
fn each_lip_carries_the_traction_of_its_own_boundary() {
    let o = run(false, false);
    eprintln!("default: {o:?}");
    assert_eq!(
        o.doubled_loaded, 2,
        "both crack tips lie on the loaded faces"
    );
    assert_close("closed form", o.lips, o.expected);
    conserved(&o);
}

#[test]
fn with_the_switch_the_original_copy_keeps_the_load() {
    let o = run(true, false);
    let off: Vec<Real> = (0..2)
        .flat_map(|l| (0..2).map(move |d| (l, d)))
        .map(|(l, d)| o.lips[l][d] - o.expected[l][d])
        .collect();
    eprintln!(
        "switch (upstream): lips {:?}, closed form {:?}, deviation {off:?}, \
         predicted misplacement {:?}",
        o.lips, o.expected, o.misplaced
    );
    let shifted = [
        [
            o.expected[0][0] + o.misplaced[0][0],
            o.expected[0][1] + o.misplaced[0][1],
        ],
        [
            o.expected[1][0] + o.misplaced[1][0],
            o.expected[1][1] + o.misplaced[1][1],
        ],
    ];
    assert_close("upstream's misplacement", o.lips, shifted);
    let size = o.misplaced[0][0].abs().max(o.misplaced[0][1].abs());
    assert!(
        size > 0.01 * scale(),
        "the switch must misplace a load here: {size}"
    );
    conserved(&o);
}

#[test]
fn zero_then_apply_again_records_the_last_application() {
    let o = run(false, true);
    assert_close("closed form after the reset loop", o.lips, o.expected);
    conserved(&o);
}
