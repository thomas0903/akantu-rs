// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the thread-count invariant of the execution policy (D-123) on the
// explicit step of the heat transfer model of
// examples/c++/diffusion_model/heat_diffusion_explicit (lumped capacity).

//! In [`akantu_core::exec::Precision::Bitwise`] the explicit heat step gives
//! the same bits for every thread count: the temperature, its rate, the
//! internal heat flow, the law's gradient, diffusivity and `D∇u` blocks and
//! the thermal energy are compared bit for bit with the one-thread run at
//! 1, 2, 3, 4 and 8 threads, finest and automatic split, on 2-D (triangles
//! and quadrangles) and 3-D (tetrahedra and hexahedra) meshes, with a
//! constant conductivity and with a temperature-dependent one.

use akantu_core::enums::AnalysisMethod;
use akantu_core::exec::{ExecPolicy, Grain};
use akantu_core::{ElementType, GhostType, Real};
use akantu_diffusion::HeatTransferModel;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;

/// An `n × n` grid of the unit square: cells with `i + j` even as
/// quadrangles, the others as two triangles (slightly irregular inside).
fn grid2d(n: usize) -> Mesh {
    let mut m = Mesh::new(2, "square").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        let h = 1.0 / n as Real;
        for j in 0..=n {
            for i in 0..=n {
                let inner = i != 0 && j != 0 && i != n && j != n;
                let w = if inner {
                    0.04 * h * (((i * 7 + j * 3) % 11) as Real / 11.0 - 0.5)
                } else {
                    0.0
                };
                a.push_node(&[i as Real * h + w, j as Real * h - w])
                    .unwrap();
            }
        }
        let id = |i: usize, j: usize| (j * (n + 1) + i) as i32;
        for j in 0..n {
            for i in 0..n {
                if (i + j) % 2 == 0 {
                    a.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost)
                        .push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
                } else {
                    for t in [
                        [id(i, j), id(i + 1, j), id(i + 1, j + 1)],
                        [id(i, j), id(i + 1, j + 1), id(i, j + 1)],
                    ] {
                        a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost)
                            .push_tuple(&t);
                    }
                }
            }
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

/// An `n³` grid of the unit cube: the cells with `i + j + k` even as
/// hexahedra, the others split into six tetrahedra (slightly irregular
/// inside).
fn cube3d(n: usize) -> Mesh {
    let mut m = Mesh::new(3, "cube").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        let h = 1.0 / n as Real;
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    let edge = |c: usize| c == 0 || c == n;
                    let p = if edge(i) || edge(j) || edge(k) {
                        0.0
                    } else {
                        0.05 * h * (((i * 7 + j * 3 + k * 5) % 11) as Real / 11.0 - 0.5)
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

type State = Vec<(String, Vec<u64>)>;

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// `steps` explicit steps from a hot spot held at 300 and walls at 100
/// (100 elsewhere); `variation`: the conductivity's temperature
/// coefficient (0: constant).
fn run(mesh: Mesh, variation: Real, policy: ExecPolicy, steps: usize) -> State {
    let dim = mesh.spatial_dimension();
    let conductivity = if dim == 2 {
        "[[401, 20], [20, 300]]"
    } else {
        "[[401, 20, 0], [20, 300, 10], [0, 10, 350]]"
    };
    let dat = format!(
        "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    name = mat\n    \
         density = 8940\n    capacity = 385\n    conductivity = {conductivity}\n    \
         conductivity_variation = {variation}\n    temperature_reference = 100\n  ]\n]\n"
    );
    let parser = Parser::parse_str(&dat, "material.dat").unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat", parser).unwrap();
    m.set_exec_policy(policy).unwrap();
    assert_eq!(m.exec_policy(), policy);
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let coords: Vec<Real> = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let nb = coords.len() / dim;
    let mut t = vec![100.0; nb];
    let mut b = vec![false; nb];
    for (i, x) in coords.chunks_exact(dim).enumerate() {
        let d = ((x[0] - 0.3).powi(2) + (x[1] - 0.6).powi(2)).sqrt();
        if d < 0.25 {
            b[i] = true;
            t[i] = 300.0;
        }
        if x.iter().any(|&v| v == 0.0 || v == 1.0) {
            b[i] = true;
        }
    }
    m.temperature_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&t);
    m.blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&b);
    let dt = m.stable_time_step().unwrap();
    m.set_time_step(0.8 * dt, None).unwrap();
    for _ in 0..steps {
        m.solve_step(None).unwrap();
    }
    let mut out = vec![
        ("T".into(), bits(m.temperature().unwrap().as_slice())),
        ("dT".into(), bits(m.temperature_rate().unwrap().as_slice())),
        (
            "q_int".into(),
            bits(m.internal_heat_rate().unwrap().as_slice()),
        ),
    ];
    for (i, law) in m.physics.laws.laws().iter().enumerate() {
        let base = law.base();
        let types = base.filter_types(GhostType::NotGhost);
        assert!(!types.is_empty(), "law {i} has elements");
        for ty in types {
            for (name, f) in [
                ("grad_u", &base.grad_u),
                ("d_grad_u", &base.d_grad_u),
                ("diffusivity", &base.diffusivity),
            ] {
                out.push((
                    format!("law{i}/{name}/{ty:?}"),
                    bits(f.get(ty, GhostType::NotGhost).unwrap().as_slice()),
                ));
                if f.has_history() {
                    out.push((
                        format!("law{i}/{name}/{ty:?}/previous"),
                        bits(f.previous(ty, GhostType::NotGhost).unwrap().as_slice()),
                    ));
                }
            }
        }
    }
    out.push((
        "thermal energy".into(),
        vec![m.energy("thermal").unwrap().to_bits()],
    ));
    out
}

fn check(name: &str, mesh: fn() -> Mesh, variation: Real) {
    let reference = run(mesh(), variation, ExecPolicy::serial(), 5);
    let moved = reference
        .iter()
        .find(|(n, _)| n == "q_int")
        .is_some_and(|(_, v)| v.iter().any(|&b| b != 0 && b != (-0.0f64).to_bits()));
    assert!(moved, "{name}: the internal heat flow is not zero");
    for threads in [1, 2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            let p = ExecPolicy::serial()
                .with_threads(threads)
                .unwrap()
                .with_grain(grain);
            let what = format!("{name}: threads={threads} {grain:?}");
            let s = run(mesh(), variation, p, 5);
            assert_eq!(reference.len(), s.len(), "{what}");
            for ((n, r), (_, o)) in reference.iter().zip(&s) {
                assert!(!r.is_empty(), "{what}: {n} empty");
                assert!(r == o, "{what}: {n} differs from the one-thread run");
            }
        }
    }
}

#[test]
fn explicit_heat_step_is_bitwise_for_every_thread_count() {
    check("2-D, constant conductivity", || grid2d(8), 0.0);
    check(
        "2-D, temperature-dependent conductivity",
        || grid2d(8),
        0.01,
    );
    check("3-D, constant conductivity", || cube3d(4), 0.0);
    check(
        "3-D, temperature-dependent conductivity",
        || cube3d(4),
        0.01,
    );
}
