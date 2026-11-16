// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the thread-count invariant of the execution policy (D-123) on the
// structural model of test/test_model/test_structural_mechanics_model/
// (Bernoulli beams: stiffness and mass assembly, static and Newmark steps).

//! In [`akantu_core::exec::Precision::Bitwise`] the structural model gives
//! the same bits for every thread count: a 3-D frame of Bernoulli beams
//! (base clamped, loads on the top nodes) whose stiffness is assembled
//! several times (the cached, threaded scatter) before a static solve, and
//! implicit dynamic steps with the consistent mass; `K`, `M`, `J` and the
//! nodal fields are compared bit for bit with the one-thread run at 2, 3,
//! 4 and 8 threads, finest and automatic split.

use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_core::exec::{ExecPolicy, Grain};
use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::param::ParamValue;
use akantu_solver::ModelSolverOptions;
use akantu_structural::{StructuralMaterial, StructuralMechanicsModel};

const NDOF: usize = 6;

/// Frame of `n³` cells (beams along the three axes, edge length 1); the
/// mesh and one normal per beam.
fn frame(n: usize) -> (Mesh, Vec<Real>) {
    let mut mesh = Mesh::new(3, "frame").unwrap();
    let id = |i: usize, j: usize, k: usize| ((k * (n + 1) + j) * (n + 1) + i) as Idx;
    let mut conn: Vec<Idx> = Vec::new();
    let mut normals: Vec<Real> = Vec::new();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    acc.push_node(&[i as Real, j as Real, k as Real]).unwrap();
                }
            }
        }
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    if i < n {
                        conn.extend([id(i, j, k), id(i + 1, j, k)]);
                        normals.extend([0.0, 0.0, 1.0]);
                    }
                    if j < n {
                        conn.extend([id(i, j, k), id(i, j + 1, k)]);
                        normals.extend([0.0, 0.0, 1.0]);
                    }
                    if k < n {
                        conn.extend([id(i, j, k), id(i, j, k + 1)]);
                        normals.extend([1.0, 0.0, 0.0]);
                    }
                }
            }
        }
        let mut c = acc.connectivity_mut(ElementType::BernoulliBeam3, GhostType::NotGhost);
        for e in conn.as_chunks::<2>().0 {
            c.push_tuple(e);
        }
        drop(c);
        acc.make_ready().unwrap();
    }
    (mesh, normals)
}

type State = Vec<(String, Vec<u64>)>;

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn model(policy: ExecPolicy, method: AnalysisMethod) -> StructuralMechanicsModel {
    let n = 3;
    let (mesh, normals) = frame(n);
    let coords = mesh.nodes().as_slice().to_vec();
    let mut m = StructuralMechanicsModel::new(SharedMesh::new(mesh), None, "structural").unwrap();
    m.set_exec_policy(policy).unwrap();
    assert_eq!(m.exec_policy(), policy);
    m.set_extra_normals(ElementType::BernoulliBeam3, &normals)
        .unwrap();
    m.add_material(
        StructuralMaterial {
            e: 2.1e11,
            a: 1e-2,
            iy: 1e-5,
            iz: 1e-5,
            gj: 1e5,
            rho: 7800.0,
            ..StructuralMaterial::default()
        },
        "",
    )
    .unwrap();
    m.init_full(
        &ModelOptions::new(method).with_solver_options(ModelSolverOptions {
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        }),
    )
    .unwrap();
    let top = n as Real;
    {
        let mut b = m.blocked_dofs_mut().unwrap();
        for (node, x) in coords.as_chunks::<3>().0.iter().enumerate() {
            if x[2] == 0.0 {
                b.as_mut_slice()[node * NDOF..(node + 1) * NDOF].fill(true);
            }
        }
    }
    {
        let mut f = m.external_force_mut().unwrap();
        for (node, x) in coords.as_chunks::<3>().0.iter().enumerate() {
            if x[2] == top {
                f.as_mut_slice()[node * NDOF] = 1e3 * (1.0 + x[0]);
                f.as_mut_slice()[node * NDOF + 2] = -1e3 * (1.0 + x[1]);
            }
        }
    }
    m
}

fn state(m: &StructuralMechanicsModel, matrices: &[&str], dynamic: bool) -> State {
    let mut out = Vec::new();
    for id in matrices {
        let a = m.dof_manager().matrix(id).unwrap();
        let (i, j, v) = a.coo();
        out.push((
            format!("{id}/coordinates"),
            i.iter().chain(j).map(|&g| g as u64).collect(),
        ));
        out.push((format!("{id}/values"), bits(v)));
    }
    out.push(("u".into(), bits(m.displacement().unwrap().as_slice())));
    if dynamic {
        out.push(("v".into(), bits(m.velocity().unwrap().as_slice())));
        out.push(("a".into(), bits(m.acceleration().unwrap().as_slice())));
    }
    out.push(("f_int".into(), bits(m.internal_force().unwrap().as_slice())));
    out
}

fn run_static(policy: ExecPolicy) -> (State, u64) {
    let mut m = model(policy, AnalysisMethod::Static);
    for _ in 0..3 {
        m.invalidate_matrices();
        m.assemble_stiffness_matrix().unwrap();
    }
    m.solve_step(None).unwrap();
    (
        state(&m, &["K"], false),
        m.dof_manager().threaded_assemblies(),
    )
}

fn run_dynamic(policy: ExecPolicy) -> State {
    let mut m = model(policy, AnalysisMethod::ImplicitDynamic);
    m.set_time_step(1e-4, None).unwrap();
    {
        let nls = m.non_linear_solver_mut(None).unwrap();
        nls.set_param("max_iterations", ParamValue::Int(10))
            .unwrap();
        nls.set_param("threshold", ParamValue::Real(1e-8)).unwrap();
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
            .unwrap();
    }
    for _ in 0..4 {
        m.solve_step(None).unwrap();
    }
    state(&m, &["K", "M", "J"], true)
}

fn compare(reference: &State, other: &State, what: &str) {
    assert_eq!(reference.len(), other.len(), "{what}");
    for ((name, r), (_, o)) in reference.iter().zip(other) {
        assert!(!r.is_empty(), "{what}: {name} empty");
        assert!(r == o, "{what}: {name} differs from the one-thread run");
    }
}

#[test]
fn structural_matrices_and_steps_are_bitwise_for_every_thread_count() {
    let (static_ref, batches) = run_static(ExecPolicy::serial());
    assert_eq!(batches, 0);
    let dynamic_ref = run_dynamic(ExecPolicy::serial());
    for threads in [2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            let p = ExecPolicy::serial()
                .with_threads(threads)
                .unwrap()
                .with_grain(grain);
            let what = format!("threads={threads} {grain:?}");
            let (s, batches) = run_static(p);
            assert!(batches > 0, "{what}: the cached assembly ran on threads");
            compare(&static_ref, &s, &format!("static {what}"));
            compare(&dynamic_ref, &run_dynamic(p), &format!("dynamic {what}"));
        }
    }
}
