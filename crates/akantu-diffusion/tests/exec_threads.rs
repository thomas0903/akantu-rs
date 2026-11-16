// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the thread-count invariant of the execution policy (D-123) on the heat
// transfer model of examples/c++/diffusion_model/ (conductivity and
// capacity matrices, implicit and static solves).

//! In [`akantu_core::exec::Precision::Bitwise`] the heat-transfer model
//! gives the same bits for every thread count: a backward-Euler run whose
//! conductivity depends on the temperature (the conductivity matrix is
//! assembled again at every Newton iteration, through the cached threaded
//! scatter) with the consistent capacity, on a 2-D mesh of triangles and
//! quadrangles with Dirichlet conditions; `K`, `M`, `J` and the nodal
//! fields are compared bit for bit with the one-thread run at 2, 3, 4 and
//! 8 threads, finest and automatic split.

use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_core::exec::{ExecPolicy, Grain};
use akantu_core::{ElementType, GhostType, Real};
use akantu_diffusion::HeatTransferModel;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::ModelSolverOptions;

/// An `n × n` grid of the unit square: cells with `i + j` even as
/// quadrangles, the others as two triangles (slightly irregular inside).
fn grid(n: usize) -> Mesh {
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

type State = Vec<(String, Vec<u64>)>;

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn run(policy: ExecPolicy) -> (State, u64) {
    let dat = "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    name = mat\n    \
               density = 8940\n    capacity = 385\n    conductivity = [[401, 20], [20, 300]]\n    \
               conductivity_variation = 0.01\n    temperature_reference = 100\n  ]\n]\n";
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(grid(8)), None, "heat", parser).unwrap();
    m.set_exec_policy(policy).unwrap();
    assert_eq!(m.exec_policy(), policy);
    m.init_full(
        &ModelOptions::new(AnalysisMethod::ImplicitDynamic).with_solver_options(
            ModelSolverOptions {
                sparse_solver_type: Some(SparseSolverType::Eigen),
                ..ModelSolverOptions::default()
            },
        ),
    )
    .unwrap();
    // a hot spot held at 300, the walls at 100, 100 elsewhere
    let coords: Vec<Real> = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let nb = coords.len() / 2;
    let mut t = vec![100.0; nb];
    let mut b = vec![false; nb];
    for (i, x) in coords.as_chunks::<2>().0.iter().enumerate() {
        let d = ((x[0] - 0.3).powi(2) + (x[1] - 0.6).powi(2)).sqrt();
        if d < 0.2 {
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
    m.set_time_step(20.0 * dt, None).unwrap();
    {
        let nls = m.solver.non_linear_solver_mut(None).unwrap();
        nls.set_param("max_iterations", ParamValue::Int(30))
            .unwrap();
        nls.set_param("threshold", ParamValue::Real(1e-10)).unwrap();
    }
    for _ in 0..3 {
        m.solve_step(None).unwrap();
    }
    let mut out = Vec::new();
    for id in ["K", "M", "J"] {
        let a = m.solver.dof_manager.matrix(id).unwrap();
        let (i, j, v) = a.coo();
        out.push((
            format!("{id}/coordinates"),
            i.iter().chain(j).map(|&g| g as u64).collect(),
        ));
        out.push((format!("{id}/values"), bits(v)));
    }
    out.push(("T".into(), bits(m.temperature().unwrap().as_slice())));
    out.push(("dT".into(), bits(m.temperature_rate().unwrap().as_slice())));
    out.push((
        "q_int".into(),
        bits(m.internal_heat_rate().unwrap().as_slice()),
    ));
    (out, m.solver.dof_manager.threaded_assemblies())
}

#[test]
fn heat_transfer_matrices_and_steps_are_bitwise_for_every_thread_count() {
    let (reference, batches) = run(ExecPolicy::serial());
    assert_eq!(batches, 0);
    for threads in [2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            let p = ExecPolicy::serial()
                .with_threads(threads)
                .unwrap()
                .with_grain(grain);
            let what = format!("threads={threads} {grain:?}");
            let (s, batches) = run(p);
            assert!(batches > 0, "{what}: the cached assembly ran on threads");
            assert_eq!(reference.len(), s.len());
            for ((name, r), (_, o)) in reference.iter().zip(&s) {
                assert!(!r.is_empty(), "{name} empty");
                assert!(r == o, "{what}: {name} differs from the one-thread run");
            }
        }
    }
}
