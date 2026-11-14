// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests
// (SMM-015, UD-209): node removal under a live PhaseFieldModel (upstream
// phase_field_model.cc has no onNodesRemoved; dof_manager.cc:901-902 is
// empty; mesh_utils.cc purifyMesh, mesh.cc eraseElements).

//! Criterion: a removal between two static solves equals a fresh model on
//! the mesh after the removal with the state transferred — the damage
//! family (values, blocked flags) moved with the removal's numbering and
//! every internal of the law (current and previous values: the strain, the
//! history `phi`, …) of the kept elements, all taken from the state before
//! the removal (computed by the test, independently of the port's
//! compaction) — compared **bitwise**: the renumbering reproduces the fresh registration and the
//! erased elements are the last ones (no element moves, the law filter
//! keeps the fresh order). The change between the solves: the last cell
//! (two triangles) erased, its orphaned corner node removed and every other
//! node renumbered by first appearance (`purify` of a mesh whose nodes are
//! created top row first), in one mutation.

mod common;

use akantu_core::{Array, Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor, MeshPart, MeshTransaction, SharedMesh};
use akantu_phase_field::PhaseFieldModel;
use common::{init_static, model_with};

const AT2: &str =
    "phasefield quadratic [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.2\n]\n";

/// `n × n` cells of the unit square, two triangles each, nodes created top
/// row first.
fn scrambled_tri_mesh(n: usize) -> Mesh {
    let mut mesh = Mesh::new(2, "tris").unwrap();
    let mut index = vec![0 as Idx; (n + 1) * (n + 1)];
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for j in (0..=n).rev() {
            for i in 0..=n {
                index[j * (n + 1) + i] = a
                    .push_node(&[i as Real / n as Real, j as Real / n as Real])
                    .unwrap();
            }
        }
        let id = |i: usize, j: usize| index[j * (n + 1) + i];
        let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for j in 0..n {
            for i in 0..n {
                c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                c.push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
    }
    mesh.compute_bounding_box().unwrap();
    mesh
}

fn change(tx: &mut MeshTransaction<'_>) -> akantu_core::Result<()> {
    let ne = tx
        .mesh()
        .nb_element(ElementType::Triangle3, GhostType::NotGhost);
    let last: Vec<Element> = (ne - 2..ne)
        .map(|e| Element::new(ElementType::Triangle3, e as Idx, GhostType::NotGhost))
        .collect();
    tx.erase_elements(MeshPart::Main, &last)?;
    tx.purify(MeshPart::Main)
}

/// Strain `ε(x)` at the quadrature points of the law (a field varying in
/// space, so that the damage varies from node to node).
fn set_strain(model: &mut PhaseFieldModel, scale: Real) {
    let mesh_h = model.mesh_handle().clone();
    let mesh = mesh_h.read().unwrap();
    let ty = ElementType::Triangle3;
    let filter = model
        .phase_field(0)
        .unwrap()
        .base()
        .filter(ty, GhostType::NotGhost)
        .to_vec();
    let mut coords = Array::new(2);
    model
        .fe_engine()
        .unwrap()
        .integration_points_coordinates(&mesh, &mut coords, ty, GhostType::NotGhost, Some(&filter))
        .unwrap();
    let law = model.phase_field_mut(0).unwrap();
    let mut s = law
        .base_mut()
        .strain
        .get_mut(ty, GhostType::NotGhost)
        .unwrap();
    for (eps, x) in s
        .as_mut_slice()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(coords.as_slice().as_chunks::<2>().0)
    {
        let e = scale * (2e-3 + 3e-3 * x[0] * x[0] + 1e-3 * x[1]);
        eps.copy_from_slice(&[0.2 * e, 0.1 * e, 0.1 * e, e]);
    }
}

/// Mutation of the model's mesh with the model as its consumer
/// (`PhaseFieldModel::mutate_mesh`). Returns the node
/// numbering of the removal.
fn mutate(model: &mut PhaseFieldModel) -> Vec<Idx> {
    model
        .mutate_mesh(change, None)
        .unwrap()
        .events
        .iter()
        .find_map(|(p, e)| match e {
            akantu_mesh::MeshEvent::NodesRemoved { new_numbering, .. } if *p == MeshPart::Main => {
                Some(new_numbering.clone())
            }
            _ => None,
        })
        .unwrap()
}

/// The state of a model before the removal: damage, blocked flags and
/// every law internal (current and previous values).
struct State {
    damage: Vec<Real>,
    blocked: Vec<bool>,
    internals: Vec<(Vec<Real>, Option<Vec<Real>>)>,
}

fn snapshot(m: &PhaseFieldModel) -> State {
    let ty = ElementType::Triangle3;
    State {
        damage: m.damage().unwrap().as_slice().to_vec(),
        blocked: m
            .dof_manager()
            .blocked_dofs("damage")
            .unwrap()
            .as_slice()
            .to_vec(),
        internals: m
            .phase_field(0)
            .unwrap()
            .base()
            .internals()
            .into_iter()
            .map(|(_, f)| {
                if !f.exists(ty, GhostType::NotGhost) {
                    return (Vec::new(), None);
                }
                let v = f.get(ty, GhostType::NotGhost).unwrap().as_slice().to_vec();
                let p = f.has_history().then(|| {
                    f.previous(ty, GhostType::NotGhost)
                        .unwrap()
                        .as_slice()
                        .to_vec()
                });
                (v, p)
            })
            .collect(),
    }
}

/// The transfer to the fresh model, computed by the test from the state
/// before the removal: nodal values moved with `numbering`
/// (`removeNodesFromArray`), element values of the kept elements (the two
/// erased ones are the last: truncation).
fn transfer(pre: &State, numbering: &[Idx], b: &mut PhaseFieldModel) {
    let kept = numbering.iter().filter(|&&n| n >= 0).count();
    let mut d = vec![0.0; kept];
    let mut bl = vec![false; kept];
    for (i, &n) in numbering.iter().enumerate() {
        if let Ok(n) = usize::try_from(n) {
            d[n] = pre.damage[i];
            bl[n] = pre.blocked[i];
        }
    }
    b.damage_mut().unwrap().as_mut_slice().copy_from_slice(&d);
    b.blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&bl);
    let ty = ElementType::Triangle3;
    let dst = b.phase_field_mut(0).unwrap();
    for ((name, fb), (v, p)) in dst
        .base_mut()
        .internals_mut()
        .into_iter()
        .zip(&pre.internals)
    {
        if !fb.exists(ty, GhostType::NotGhost) {
            continue;
        }
        let mut g = fb.get_mut(ty, GhostType::NotGhost).unwrap();
        let len = g.as_slice().len();
        g.as_mut_slice().copy_from_slice(&v[..len]);
        drop(g);
        if let Some(p) = p {
            let q = fb
                .previous_mut(ty, GhostType::NotGhost)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let len = q.as_slice().len();
            q.as_mut_slice().copy_from_slice(&p[..len]);
        }
    }
}

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn static_removal_equals_a_fresh_model() {
    let n = 4;
    let mut a = model_with(scrambled_tri_mesh(n), AT2);
    init_static(&mut a);
    set_strain(&mut a, 1.0);
    a.solve_step(None).unwrap();
    a.save_previous_state().unwrap();
    let nodes_before = a.mesh_handle().read().unwrap().nb_nodes();
    let pre = snapshot(&a);
    let numbering = mutate(&mut a);
    assert_eq!(a.mesh_handle().read().unwrap().nb_nodes(), nodes_before - 1);
    assert!(
        numbering
            .iter()
            .enumerate()
            .any(|(i, &n)| n >= 0 && n as usize != i)
    );
    // the damage followed its nodes
    let mut want = vec![0.0; nodes_before - 1];
    for (i, &n) in numbering.iter().enumerate() {
        if let Ok(n) = usize::try_from(n) {
            want[n] = pre.damage[i];
        }
    }
    assert_eq!(bits(a.damage().unwrap().as_slice()), bits(&want));
    set_strain(&mut a, 0.5); // unloading: the history keeps the damage
    // B: the same mesh through the same change without consumers
    let shared_b = SharedMesh::new(scrambled_tri_mesh(n));
    shared_b.mutate(&mut [], change, None).unwrap();
    let parser = akantu_parser::Parser::parse_str(AT2, "material.dat").unwrap();
    let mut b = PhaseFieldModel::with_parser(shared_b, None, "phase_field_model", parser).unwrap();
    init_static(&mut b);
    transfer(&pre, &numbering, &mut b);
    set_strain(&mut b, 0.5);
    assert_eq!(a.dof_manager().layout(), b.dof_manager().layout());
    assert_eq!(
        a.dof_manager().local_equation_numbers("damage").unwrap(),
        b.dof_manager().local_equation_numbers("damage").unwrap()
    );
    a.solve_step(None).unwrap();
    b.solve_step(None).unwrap();
    let (da, db) = (
        a.damage().unwrap().as_slice().to_vec(),
        b.damage().unwrap().as_slice().to_vec(),
    );
    assert!(da.iter().any(|&d| d > 0.0) && da.iter().any(|&d| d != da[0]));
    assert_eq!(bits(&da), bits(&db), "damage");
    for ((name, fa), (_, fb)) in a
        .phase_field(0)
        .unwrap()
        .base()
        .internals()
        .into_iter()
        .zip(b.phase_field(0).unwrap().base().internals())
    {
        let ty = ElementType::Triangle3;
        if fa.exists(ty, GhostType::NotGhost) {
            assert_eq!(
                bits(fa.get(ty, GhostType::NotGhost).unwrap().as_slice()),
                bits(fb.get(ty, GhostType::NotGhost).unwrap().as_slice()),
                "{name}"
            );
        }
    }
}

/// UD-1071: the model's nodal forces follow their nodes
/// through a removal (upstream `PhaseFieldModel` has no `onNodesRemoved`:
/// its forces keep the old node count). Distinct values per node before the
/// removal, the moved values after it (a truncation instead of the move
/// fails here: the purify numbering is not the identity on the kept nodes).
#[test]
fn forces_follow_their_nodes_through_a_removal() {
    let mut a = model_with(scrambled_tri_mesh(4), AT2);
    init_static(&mut a);
    let nodes_before = a.mesh_handle().read().unwrap().nb_nodes();
    let ext: Vec<Real> = (0..nodes_before).map(|i| 1000.0 + i as Real).collect();
    let int: Vec<Real> = (0..nodes_before).map(|i| -2000.0 - i as Real).collect();
    a.external_force_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&ext);
    a.internal_force_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&int);
    let numbering = mutate(&mut a);
    let kept = numbering.iter().filter(|&&n| n >= 0).count();
    assert!(kept < nodes_before);
    let (mut want_ext, mut want_int) = (vec![0.0; kept], vec![0.0; kept]);
    for (i, &n) in numbering.iter().enumerate() {
        if let Ok(n) = usize::try_from(n) {
            want_ext[n] = ext[i];
            want_int[n] = int[i];
        }
    }
    assert_ne!(want_ext[..], ext[..kept], "a non-trivial renumbering");
    assert_eq!(a.external_force().unwrap().as_slice(), &want_ext[..]);
    assert_eq!(a.internal_force().unwrap().as_slice(), &want_int[..]);
}

/// Port defect: an erasure in one block used
/// to reset the law maps of every block; the phase-field model now keeps
/// the maps of the blocks the erasure does not renumber (upstream resizes
/// those of the renumbered types only).
#[test]
fn erasure_keeps_the_law_maps_of_the_other_blocks() {
    let mut mesh = Mesh::new(2, "mixed").unwrap();
    {
        let mut m = MeshAccessor::new(&mut mesh);
        for p in [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [2.0, 0.0],
            [2.0, 1.0],
        ] {
            m.push_node(&p as &[Real]).unwrap();
        }
        let mut c = m.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        c.push_tuple(&[0, 1, 2]);
        c.push_tuple(&[0, 2, 3]);
        drop(c);
        let mut q = m.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
        q.push_tuple(&[1, 4, 5, 2]);
    }
    mesh.compute_bounding_box().unwrap();
    let mut a = model_with(mesh, AT2);
    init_static(&mut a);
    let quad = |a: &PhaseFieldModel| {
        let ty = ElementType::Quadrangle4;
        (
            a.phase_field_by_element()
                .get(ty, GhostType::NotGhost)
                .unwrap()
                .as_slice()
                .to_vec(),
            a.phase_field_local_numbering()
                .get(ty, GhostType::NotGhost)
                .unwrap()
                .as_slice()
                .to_vec(),
        )
    };
    let before = quad(&a);
    assert_eq!(before, (vec![0], vec![0]));
    a.mutate_mesh(
        |tx| {
            tx.erase_elements(
                MeshPart::Main,
                &[Element::new(ElementType::Triangle3, 0, GhostType::NotGhost)],
            )
        },
        None,
    )
    .unwrap();
    assert_eq!(quad(&a), before);
    let tri = a
        .phase_field_by_element()
        .get(ElementType::Triangle3, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(tri, vec![0]);
}
