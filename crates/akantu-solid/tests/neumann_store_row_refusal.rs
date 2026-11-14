// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS).
// The Neumann store's capability check of a node event (a boundary row the
// records cannot follow by node identity) at the model level, through a mesh
// mutation. Mesh: upstream's test_cohesive/_cohesive_2d_4_triangle_3.msh;
// Neumann application of src/model/common/boundary_condition/
// boundary_condition_tmpl.hh.

mod model_common;

use akantu_core::Idx;
use akantu_core::enums::AnalysisMethod;
use akantu_mesh::{MeshEvent, MeshPart};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FromTraction;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use model_common::{model_with, read_mesh};

const MATERIAL: &str = "material elastic [\n\
\t name = body\n\
\t rho = 1e3\n\
\t E   = 1e9\n\
\t nu  = 0.2\n\
]\n";

fn model() -> SolidMechanicsModel {
    let p = fixtures_root().join(
        "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_cohesive/\
         _cohesive_2d_4_triangle_3.msh",
    );
    let mut m = model_with(read_mesh(&p, 2), MATERIAL);
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    m
}

fn removal_then_addition(m: &mut SolidMechanicsModel) -> akantu_core::Result<()> {
    m.mutate_mesh(
        |tx| {
            tx.purify(MeshPart::Main)?;
            let n = tx.push_node(MeshPart::Main, &[10.0, 10.0])?;
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes: vec![n],
                    old_nodes: None,
                    origin: "row refusal".into(),
                    extension: None,
                },
            );
            Ok(())
        },
        None,
    )
    .map(|_| ())
}

/// Without a Neumann store the mutation "purify, then append a node" (the
/// order the DOF manager asks for: removals before additions)
/// commits.
#[test]
fn without_a_store_removal_then_addition_commits() {
    let mut m = model();
    removal_then_addition(&mut m).unwrap();
}

/// With a store holding records the same mutation is refused before the
/// commit, recoverably, by the row check (UD-1280), which
/// compares the records (old numbering) with the renumbered rows;
/// the refusal is kept (a known limit) and the message names the case in
/// the message.
#[test]
fn with_a_store_removal_then_addition_is_refused_recoverably() {
    let mut m = model();
    m.apply_neumann_bc(&FromTraction::new(vec![3.0e5, -2.0e5]), "sides")
        .unwrap();
    let revision = m.physics.base.mesh.revision().unwrap();
    let r = removal_then_addition(&mut m);
    eprintln!("result: {r:?}");
    let e = r.expect_err("refused");
    assert!(
        matches!(e, akantu_core::Error::Unsupported(_)) && !e.is_fatal(),
        "{e:?}"
    );
    assert!(format!("{e}").contains("UD-1280"), "{e}");
    assert_eq!(m.physics.base.mesh.revision().unwrap(), revision);
}

const SEG: akantu_core::ElementType = akantu_core::ElementType::Segment2;
const G: akantu_core::GhostType = akantu_core::GhostType::NotGhost;

/// A loaded segment (record non-zero) of the model.
fn a_loaded_segment(m: &SolidMechanicsModel) -> akantu_core::Element {
    let st = m.neumann_store().unwrap();
    let blk = st.forces().get(SEG, G).unwrap();
    let e = (0..blk.len())
        .find(|&e| blk.tuple(e).iter().any(|v| *v != 0.0))
        .expect("a loaded segment");
    akantu_core::Element::new(SEG, e as Idx, G)
}

/// One mutation: a new node (not a copy) replaces the second node of the
/// loaded segment `el`, announced by a `NodesAdded` without `old_nodes`.
fn reconnect_to_a_new_node(
    m: &mut SolidMechanicsModel,
    el: akantu_core::Element,
) -> akantu_core::Result<akantu_mesh::CommittedMutation> {
    m.mutate_mesh(
        |tx| {
            let mut row = tx.mesh().connectivity(SEG, G)?.tuple(el.idx()).to_vec();
            let n = tx.push_node(MeshPart::Main, &[10.0, 10.0])?;
            row[1] = n;
            tx.set_element_nodes(MeshPart::Main, el, &row)?;
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes: vec![n],
                    old_nodes: None,
                    origin: "row refusal".into(),
                    extension: None,
                },
            );
            Ok(())
        },
        None,
    )
}

/// The row check's refusal at the model level: refused in the check stage, before
/// the commit (recoverable, the revision and the record unchanged), and the
/// domain usable afterwards. The mutant "the NodesAdded check removed from
/// smm_check_mesh_event" makes this a fatal dispatch error (poisoned
/// domain): this test kills it; no test of the branch does.
#[test]
fn a_row_the_record_cannot_follow_is_refused_before_the_commit() {
    let mut m = model();
    m.apply_neumann_bc(&FromTraction::new(vec![3.0e5, -2.0e5]), "sides")
        .unwrap();
    let el = a_loaded_segment(&m);
    let revision = m.physics.base.mesh.revision().unwrap();
    let record = format!("{:?}", m.neumann_store().unwrap().forces());
    let e = reconnect_to_a_new_node(&mut m, el).expect_err("refused");
    eprintln!("refusal: {e}");
    assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e:?}");
    assert!(!e.is_fatal(), "recoverable: {e:?}");
    assert!(format!("{e}").contains("UD-1280"), "{e}");
    assert_eq!(m.physics.base.mesh.revision().unwrap(), revision);
    assert_eq!(format!("{:?}", m.neumann_store().unwrap().forces()), record);
    // the domain is not poisoned
    m.mutate_mesh(|tx| tx.purify(MeshPart::Main), None).unwrap();
}

/// The refusal's advice holds. Zeroing the external force
/// before the change lets it through (the check and the node event read the
/// external force: the records of zeroed nodes no longer count as loads),
/// and applying the loads again after it is accepted, the record then held
/// at the element's new row.
/// Fails before the fix (the check does not read the external force).
#[test]
fn zeroing_the_external_force_before_the_change_lets_it_through() {
    let mut m = model();
    let traction = FromTraction::new(vec![3.0e5, -2.0e5]);
    m.apply_neumann_bc(&traction, "sides").unwrap();
    let el = a_loaded_segment(&m);
    for v in m.external_force_mut().unwrap().as_mut_slice() {
        *v = 0.0;
    }
    let r = reconnect_to_a_new_node(&mut m, el);
    eprintln!("after zeroing: {:?}", r.as_ref().map(|_| ()));
    assert!(
        r.is_ok(),
        "the change is still refused after zeroing the external force: {:?}",
        r.err()
    );
    let row = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .connectivity(SEG, G)
        .unwrap()
        .tuple(el.idx())
        .to_vec();
    m.apply_neumann_bc(&traction, "sides")
        .expect("the loads applied again after the change");
    let st = m.neumann_store().unwrap();
    assert_eq!(st.holders().get(SEG, G).unwrap().tuple(el.idx()), row);
    assert!(
        st.forces()
            .get(SEG, G)
            .unwrap()
            .tuple(el.idx())
            .iter()
            .any(|v| *v != 0.0),
        "the element's load is recorded again"
    );
}
