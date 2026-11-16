// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the thread-count invariant of the execution policy (D-123) on the
// explicit step of examples/c++/solid_mechanics_model/explicit (elastic
// law, explicit lumped mass).

//! In [`Precision::Bitwise`] the explicit step gives the same bits for every
//! thread count: every state array (displacement, velocity, acceleration,
//! internal force, the laws' gradients and stresses) is compared bit for bit
//! with the one-thread run, small strain and finite deformation, on a mesh
//! with two element types, with batches split as finely as possible
//! ([`Grain::Finest`]) so that every threaded kernel really splits.

use akantu_core::enums::AnalysisMethod;
use akantu_core::exec::{ExecPolicy, Grain};
use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshPart, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;

/// An `n³` grid of the unit cube: the cells with `i + j + k` even as
/// hexahedra, the others split into six tetrahedra.
fn mixed_cube(n: usize) -> Mesh {
    let mut m = Mesh::new(3, "cube").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        let h = 1.0 / n as Real;
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    // a slightly irregular grid (distinct Jacobians)
                    let w = 0.05 * h * (((i * 7 + j * 3 + k * 5) % 11) as Real / 11.0 - 0.5);
                    let edge = |c: usize| c == 0 || c == n;
                    let p = if edge(i) || edge(j) || edge(k) {
                        0.0
                    } else {
                        w
                    };
                    a.push_node(&[
                        i as Real * h + p,
                        j as Real * h - p,
                        k as Real * h + 0.5 * p,
                    ])
                    .unwrap();
                }
            }
        }
        let id = |i: usize, j: usize, k: usize| ((k * (n + 1) + j) * (n + 1) + i) as i32;
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    let c = [
                        id(i, j, k),
                        id(i + 1, j, k),
                        id(i + 1, j + 1, k),
                        id(i, j + 1, k),
                        id(i, j, k + 1),
                        id(i + 1, j, k + 1),
                        id(i + 1, j + 1, k + 1),
                        id(i, j + 1, k + 1),
                    ];
                    if (i + j + k) % 2 == 0 {
                        a.connectivity_mut(ElementType::Hexahedron8, GhostType::NotGhost)
                            .push_tuple(&c);
                    } else {
                        for t in [
                            [0, 1, 3, 4],
                            [1, 2, 3, 6],
                            [1, 3, 4, 6],
                            [1, 4, 5, 6],
                            [3, 4, 6, 7],
                            [3, 2, 6, 1],
                        ] {
                            let tet = [c[t[0]], c[t[1]], c[t[2]], c[t[3]]];
                            a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost)
                                .push_tuple(&tet);
                        }
                    }
                }
            }
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

fn model(n: usize, fd: bool, policy: ExecPolicy) -> SolidMechanicsModel {
    let dat = format!(
        "material elastic [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
         finite_deformation = {fd}\n]\n"
    );
    let parser = Parser::parse_str(&dat, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mixed_cube(n)), None, "smm", parser)
            .unwrap();
    m.set_exec_policy(policy).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let dt = m.stable_time_step().unwrap() * 0.8;
    m.set_time_step(dt, None).unwrap();
    let x: Vec<Real> = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    for (u, p) in m
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .as_chunks_mut::<3>()
        .0
        .iter_mut()
        .zip(x.as_chunks::<3>().0)
    {
        // a smooth field with all components (finite deformation needs
        // large gradients to differ from small strain)
        let s = if fd { 2e-2 } else { 1e-3 };
        u[0] = s * (6.0 * p[0]).sin() * (p[1] + 0.3);
        u[1] = s * (5.0 * p[2]).cos() * p[0];
        u[2] = -s * (4.0 * p[1] * p[2]).sin();
    }
    m
}

/// Every state array of the model: nodal fields, then per law and block the
/// gradient and the stress (second Piola–Kirchhoff with finite
/// deformation).
fn state(m: &SolidMechanicsModel) -> Vec<(String, Vec<u64>)> {
    let bits = |v: &[Real]| v.iter().map(|x| x.to_bits()).collect::<Vec<u64>>();
    let mut out = vec![
        ("u".into(), bits(m.displacement().unwrap().as_slice())),
        ("v".into(), bits(m.velocity().unwrap().as_slice())),
        ("a".into(), bits(m.acceleration().unwrap().as_slice())),
        ("f_int".into(), bits(m.internal_force().unwrap().as_slice())),
    ];
    for (i, law) in m.physics.materials.laws().iter().enumerate() {
        let b = law.base();
        for ty in [ElementType::Tetrahedron4, ElementType::Hexahedron8] {
            out.push((
                format!("law{i}/{ty:?}/grad_u"),
                bits(b.grad_u.get(ty, GhostType::NotGhost).unwrap().as_slice()),
            ));
            out.push((
                format!("law{i}/{ty:?}/sigma"),
                bits(
                    b.sigma_field()
                        .unwrap()
                        .get(ty, GhostType::NotGhost)
                        .unwrap()
                        .as_slice(),
                ),
            ));
        }
    }
    out
}

fn run(n: usize, fd: bool, policy: ExecPolicy, steps: usize) -> Vec<(String, Vec<u64>)> {
    let mut m = model(n, fd, policy);
    for _ in 0..steps {
        m.solve_step(None).unwrap();
    }
    state(&m)
}

fn compare(reference: &[(String, Vec<u64>)], other: &[(String, Vec<u64>)], what: &str) {
    assert_eq!(reference.len(), other.len());
    for ((name, r), (_, o)) in reference.iter().zip(other) {
        assert!(!r.is_empty(), "{name} empty");
        assert!(r == o, "{what}: {name} differs from the one-thread run");
    }
}

#[test]
fn explicit_step_is_bitwise_for_every_thread_count() {
    for fd in [false, true] {
        let serial = ExecPolicy::serial();
        let reference = run(4, fd, serial, 6);
        assert!(
            reference[0].1.iter().any(|&b| b != 0),
            "the displacement moved"
        );
        for threads in [2, 3, 4, 8] {
            for grain in [Grain::Finest, Grain::Auto] {
                let p = serial.with_threads(threads).unwrap().with_grain(grain);
                compare(
                    &reference,
                    &run(4, fd, p, 6),
                    &format!("fd={fd} threads={threads} {grain:?}"),
                );
            }
        }
    }
}

/// A connectivity change between steps (a mesh mutation, which bumps the
/// topology revision) reaches the threaded assembly: its cached node tables
/// are rebuilt, so the threaded run still equals the serial one.
#[test]
fn a_topology_change_rebuilds_the_scatter_tables() {
    let run = |policy: ExecPolicy| {
        let mut m = model(3, false, policy);
        for _ in 0..2 {
            m.solve_step(None).unwrap();
        }
        // as a cohesive insertion: a node doubled, one tetrahedron
        // re-connected to the copy (the nodal arrays grow, the bulk
        // connectivity changes)
        m.mutate_mesh(
            |tx| {
                let el = Element::new(ElementType::Tetrahedron4, 5, GhostType::NotGhost);
                let mut row = tx
                    .mesh()
                    .connectivity(ElementType::Tetrahedron4, GhostType::NotGhost)?
                    .tuple(el.idx())
                    .to_vec();
                let n = tx.duplicate_node(MeshPart::Main, row[2])?;
                let old = row[2];
                row[2] = n;
                tx.set_element_nodes(MeshPart::Main, el, &row)?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: Some(vec![old]),
                        origin: "exec_threads".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
        for _ in 0..2 {
            m.solve_step(None).unwrap();
        }
        state(&m)
    };
    let reference = run(ExecPolicy::serial());
    let threaded = ExecPolicy::serial()
        .with_threads(3)
        .unwrap()
        .with_grain(Grain::Finest);
    compare(&reference, &run(threaded), "after a connectivity change");
}

#[test]
fn the_policy_round_trips_through_the_model() {
    let p = ExecPolicy::serial().with_threads(2).unwrap();
    let m = model(1, false, p);
    assert_eq!(m.exec_policy(), p);
}
