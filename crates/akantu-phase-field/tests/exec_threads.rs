// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the thread-count invariant of the execution policy (D-123) on the
// phase-field model of test/test_model/test_phase_field_model/ (stiffness
// assembly and the static solve).

//! In [`akantu_core::exec::Precision::Bitwise`] the phase-field model gives
//! the same bits for every thread count: a static solve under a strain
//! field that varies in space, then repeated stiffness assemblies (the
//! fused `∫ Nᵀ b N` and `∫ Bᵀ D B` elemental matrices on threads, the
//! cached threaded scatter); `K` and the damage are compared bit for bit
//! with the one-thread run at 2, 3, 4 and 8 threads, finest and automatic
//! split.

mod common;

use akantu_core::exec::{ExecPolicy, Grain};
use akantu_core::{Array, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_phase_field::PhaseFieldModel;
use common::{init_static, model_with};

const AT2: &str =
    "phasefield quadratic [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.2\n]\n";

/// `n × n` cells of the unit square, two triangles each, slightly
/// irregular inside.
fn tri_mesh(n: usize) -> Mesh {
    let mut mesh = Mesh::new(2, "tris").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        let h = 1.0 / n as Real;
        for j in 0..=n {
            for i in 0..=n {
                let w = if i > 0 && j > 0 && i < n && j < n {
                    0.04 * h * (((7 * i + 3 * j) % 11) as Real / 11.0 - 0.5)
                } else {
                    0.0
                };
                a.push_node(&[i as Real * h + w, j as Real * h - w])
                    .unwrap();
            }
        }
        let id = |i: usize, j: usize| (j * (n + 1) + i) as Idx;
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

/// A strain field varying in space at the quadrature points of the law.
fn set_strain(model: &mut PhaseFieldModel) {
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
        let e = 2e-3 + 3e-3 * x[0] * x[0] + 1e-3 * x[1];
        eps.copy_from_slice(&[0.2 * e, 0.1 * e, 0.1 * e, e]);
    }
}

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

type State = Vec<(String, Vec<u64>)>;

fn run(policy: ExecPolicy) -> (State, u64) {
    let mut m = model_with(tri_mesh(10), AT2);
    m.set_exec_policy(policy).unwrap();
    assert_eq!(m.exec_policy(), policy);
    init_static(&mut m);
    set_strain(&mut m);
    m.solve_step(None).unwrap();
    for _ in 0..3 {
        m.assemble_stiffness_matrix().unwrap();
    }
    let k = m.dof_manager().matrix("K").unwrap();
    let (i, j, v) = k.coo();
    let state = vec![
        (
            "K/coordinates".to_owned(),
            i.iter().chain(j).map(|&g| g as u64).collect(),
        ),
        ("K/values".to_owned(), bits(v)),
        ("damage".to_owned(), bits(m.damage().unwrap().as_slice())),
    ];
    (state, m.dof_manager().threaded_assemblies())
}

#[test]
fn phase_field_stiffness_and_solve_are_bitwise_for_every_thread_count() {
    let (reference, batches) = run(ExecPolicy::serial());
    assert_eq!(batches, 0);
    assert!(
        reference[2].1.iter().any(|&b| b != 0),
        "the damage is not zero"
    );
    for threads in [2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            let p = ExecPolicy::serial()
                .with_threads(threads)
                .unwrap()
                .with_grain(grain);
            let what = format!("threads={threads} {grain:?}");
            let (s, batches) = run(p);
            assert!(batches > 0, "{what}: the cached assembly ran on threads");
            for ((name, r), (_, o)) in reference.iter().zip(&s) {
                assert!(r == o, "{what}: {name} differs from the one-thread run");
            }
        }
    }
}
