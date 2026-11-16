// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the thread-count invariant of the execution policy (D-123) on the
// implicit paths of examples/c++/solid_mechanics_model/static and
// .../implicit (stiffness and consistent mass assembly, Newton steps).

//! In [`Precision::Bitwise`] the matrix assembly and the implicit steps give
//! the same bits for every thread count: `K`, `M` and `J` (coordinates and
//! values), the displacement, velocity, acceleration, internal force and
//! the laws' gradients and stresses are compared bit for bit with the
//! one-thread run at 2, 3, 4 and 8 threads, finest and automatic split:
//! a static Newton solve with two laws on one mesh of two element types
//! (an elastic law, whose tangent runs on threads, and `neohookean`, which
//! did not opt in and whose finite-deformation tangent changes at every
//! iteration, so `K` is assembled again and again through the cached,
//! threaded scatter) under Dirichlet conditions with prescribed values; an
//! implicit dynamic run with the consistent mass; and repeated stiffness
//! assemblies of an elastic model.

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_core::exec::{ExecPolicy, Grain};
use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::{ParamValue, Parser};
use akantu_solid::SolidMechanicsModel;
use akantu_solver::ModelSolverOptions;

/// An `n³` grid of the unit cube (slightly irregular inside): the cells
/// with `i + j + k` even as hexahedra, the others as six tetrahedra.
fn mixed_cube(n: usize) -> Mesh {
    let mut m = Mesh::new(3, "cube").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        let h = 1.0 / n as Real;
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    let edge = |c: usize| c == 0 || c == n;
                    let w = if edge(i) || edge(j) || edge(k) {
                        0.0
                    } else {
                        0.05 * h * (((i * 7 + j * 3 + k * 5) % 11) as Real / 11.0 - 0.5)
                    };
                    a.push_node(&[
                        i as Real * h + w,
                        j as Real * h - w,
                        k as Real * h + 0.5 * w,
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
                            a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost)
                                .push_tuple(&[c[t[0]], c[t[1]], c[t[2]], c[t[3]]]);
                        }
                    }
                }
            }
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

/// Elements whose index is a multiple of three take law 1, the others
/// law 0.
#[derive(Debug)]
struct EveryThird;

impl akantu_model::constitutive_law::ConstitutiveLawSelector for EveryThird {
    fn select(
        &self,
        _: &akantu_model::constitutive_law::SelectorContext<'_>,
        e: Element,
    ) -> akantu_core::Result<Option<usize>> {
        Ok(Some(usize::from(e.index() % 3 == 0)))
    }
}

fn options(method: AnalysisMethod, nls: Option<NonLinearSolverType>) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: nls,
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

type State = Vec<(String, Vec<u64>)>;

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// The model's matrices (rows, columns and values of each), nodal arrays
/// and per law and type of its filter the gradient and the stress.
fn state(m: &SolidMechanicsModel, matrices: &[&str]) -> State {
    let mut out = Vec::new();
    for id in matrices {
        let a = m.solver.dof_manager.matrix(id).unwrap();
        let (i, j, v) = a.coo();
        out.push((
            format!("{id}/coordinates"),
            i.iter().chain(j).map(|&g| g as u64).collect(),
        ));
        out.push((format!("{id}/values"), bits(v)));
    }
    out.push(("u".into(), bits(m.displacement().unwrap().as_slice())));
    if let Ok(v) = m.velocity() {
        out.push(("v".into(), bits(v.as_slice())));
    }
    if let Ok(a) = m.acceleration() {
        out.push(("a".into(), bits(a.as_slice())));
    }
    out.push(("f_int".into(), bits(m.internal_force().unwrap().as_slice())));
    for (i, law) in m.physics.materials.laws().iter().enumerate() {
        let b = law.base();
        for ty in b.filter_types(GhostType::NotGhost) {
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

fn compare(reference: &State, other: &State, what: &str) {
    assert_eq!(reference.len(), other.len(), "{what}");
    for ((name, r), (_, o)) in reference.iter().zip(other) {
        assert!(!r.is_empty(), "{what}: {name} empty");
        assert!(r == o, "{what}: {name} differs from the one-thread run");
    }
}

fn policies() -> Vec<(ExecPolicy, String)> {
    let mut v = Vec::new();
    for threads in [2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            let p = ExecPolicy::serial()
                .with_threads(threads)
                .unwrap()
                .with_grain(grain);
            v.push((p, format!("threads={threads} {grain:?}")));
        }
    }
    v
}

/// Clamp `x = 0` (all components at a non-zero prescribed value) and
/// prescribe the first component on `x = 1`; returns the latter's DOFs.
fn clamp_and_pull(m: &mut SolidMechanicsModel) -> Vec<usize> {
    let x: Vec<Real> = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let mut pulled = Vec::new();
    let mut blocked = vec![false; x.len()];
    {
        let mut u = m.displacement_mut().unwrap();
        let u = u.as_mut_slice();
        for (n, p) in x.as_chunks::<3>().0.iter().enumerate() {
            if p[0] < 1e-9 {
                for c in 0..3 {
                    u[3 * n + c] = 1e-4 * (c as Real + 1.0) * (p[1] - 0.5);
                    blocked[3 * n + c] = true;
                }
            } else if p[0] > 1.0 - 1e-9 {
                blocked[3 * n] = true;
                pulled.push(3 * n);
            }
        }
    }
    m.blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked);
    pulled
}

/// Static Newton steps, elastic + neohookean.
fn static_two_laws(policy: ExecPolicy) -> (State, u64) {
    let dat = "material elastic [\n  name = a\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n]\n\
               material neohookean [\n  name = b\n  rho = 2000\n  E = 50e9\n  nu = 0.3\n]\n";
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mixed_cube(3)), None, "smm", parser)
            .unwrap();
    m.set_exec_policy(policy).unwrap();
    m.set_material_selector(Box::new(EveryThird));
    m.init_full(&options(
        AnalysisMethod::Static,
        Some(NonLinearSolverType::NewtonRaphson),
    ))
    .unwrap();
    assert_eq!(m.physics.materials.nb_laws(), 2);
    {
        let nls = m.solver.non_linear_solver_mut(None).unwrap();
        nls.set_param("max_iterations", ParamValue::Int(30))
            .unwrap();
        nls.set_param("threshold", ParamValue::Real(1e-10)).unwrap();
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
            .unwrap();
    }
    let pulled = clamp_and_pull(&mut m);
    for _ in 0..3 {
        {
            let mut u = m.displacement_mut().unwrap();
            for &k in &pulled {
                u.as_mut_slice()[k] += 4e-3;
            }
        }
        m.solve_step(None).unwrap();
    }
    (
        state(&m, &["K", "J"]),
        m.solver.dof_manager.threaded_assemblies(),
    )
}

#[test]
fn static_newton_with_a_serial_law_is_bitwise_for_every_thread_count() {
    let (reference, batches) = static_two_laws(ExecPolicy::serial());
    assert_eq!(batches, 0);
    for (p, what) in policies() {
        let (s, batches) = static_two_laws(p);
        assert!(batches > 0, "{what}: the cached assembly ran on threads");
        compare(&reference, &s, &what);
    }
}

/// Implicit dynamic steps (trapezoidal rule) of an elastic model with the
/// consistent mass.
fn implicit_dynamic(policy: ExecPolicy) -> State {
    let dat = "material elastic [\n  name = a\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n]\n";
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mixed_cube(3)), None, "smm", parser)
            .unwrap();
    m.set_exec_policy(policy).unwrap();
    m.init_full(&options(AnalysisMethod::ImplicitDynamic, None))
        .unwrap();
    m.assemble_mass().unwrap();
    let dt = m.stable_time_step().unwrap();
    m.set_time_step(4.0 * dt, None).unwrap();
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
        u[0] = 1e-3 * (6.0 * p[0]).sin() * (p[1] + 0.3);
        u[1] = 1e-3 * (5.0 * p[2]).cos() * p[0];
        u[2] = -1e-3 * (4.0 * p[1] * p[2]).sin();
    }
    for _ in 0..4 {
        m.solve_step(None).unwrap();
    }
    state(&m, &["K", "M", "J"])
}

#[test]
fn implicit_dynamics_with_consistent_mass_is_bitwise_for_every_thread_count() {
    let reference = implicit_dynamic(ExecPolicy::serial());
    for (p, what) in policies() {
        compare(&reference, &implicit_dynamic(p), &what);
    }
}

/// Repeated forced stiffness assemblies (first: profile, second: positions
/// stored, then cached) of an elastic model, `K` after each.
fn repeated_stiffness(policy: ExecPolicy) -> (Vec<State>, u64) {
    let dat = "material elastic [\n  name = a\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n]\n\
               material elastic [\n  name = b\n  rho = 2000\n  E = 30e9\n  nu = 0.3\n]\n";
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mixed_cube(3)), None, "smm", parser)
            .unwrap();
    m.set_exec_policy(policy).unwrap();
    m.set_material_selector(Box::new(EveryThird));
    m.init_full(&options(AnalysisMethod::Static, None)).unwrap();
    let mut out = Vec::new();
    for _ in 0..4 {
        m.assemble_stiffness_matrix(true).unwrap();
        out.push(state(&m, &["K"]));
    }
    (out, m.solver.dof_manager.threaded_assemblies())
}

#[test]
fn repeated_stiffness_assembly_of_two_laws_is_bitwise_for_every_thread_count() {
    let (reference, _) = repeated_stiffness(ExecPolicy::serial());
    for (p, what) in policies() {
        let (s, batches) = repeated_stiffness(p);
        // two laws × two types, from the second assembly on
        assert!(batches >= 3 * 4, "{what}: {batches} threaded assemblies");
        for (r, o) in reference.iter().zip(&s) {
            compare(r, o, &what);
        }
    }
}

/// The arrays the model keeps between stiffness assemblies stay bounded:
/// after two assemblies (the pool filled), six more change neither the
/// number of pooled arrays nor the bytes held, for a small-strain law, a
/// finite-deformation law (whose blocks the pool must not keep), two laws
/// of both kinds, at one and at three threads; `release_static_solver`
/// frees them, and assembling again comes back to the same footprint.
#[test]
fn kept_assembly_arrays_stay_bounded() {
    let small = "material elastic [\n  name = a\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n]\n";
    let finite = "material elastic [\n  name = a\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
                  finite_deformation = true\n]\n";
    let two = "material elastic [\n  name = a\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n]\n\
               material neohookean [\n  name = b\n  rho = 2000\n  E = 50e9\n  nu = 0.3\n]\n";
    for (name, dat, select) in [
        ("small strain", small, false),
        ("finite deformation", finite, false),
        ("two laws", two, true),
    ] {
        for threads in [1, 3] {
            let what = format!("{name}, {threads} threads");
            let parser = Parser::parse_str(dat, "material.dat").unwrap();
            let mut m = SolidMechanicsModel::with_parser(
                SharedMesh::new(mixed_cube(3)),
                None,
                "smm",
                parser,
            )
            .unwrap();
            m.set_exec_policy(ExecPolicy::serial().with_threads(threads).unwrap())
                .unwrap();
            if select {
                m.set_material_selector(Box::new(EveryThird));
            }
            m.init_full(&options(AnalysisMethod::Static, None)).unwrap();
            for _ in 0..2 {
                m.assemble_stiffness_matrix(true).unwrap();
            }
            let filled = m.assembly_scratch_footprint();
            // a small-strain block keeps its arrays; finite-deformation
            // blocks are not pooled (only the gradient buffer is unused)
            assert_eq!(
                filled.1 > 0,
                name != "finite deformation",
                "{what}: {filled:?}"
            );
            for _ in 0..6 {
                m.assemble_stiffness_matrix(true).unwrap();
            }
            assert_eq!(
                m.assembly_scratch_footprint(),
                filled,
                "{what}: the kept arrays grew"
            );
            m.release_static_solver().unwrap();
            assert_eq!(
                m.assembly_scratch_footprint(),
                (0, 0),
                "{what}: not released"
            );
            for _ in 0..3 {
                m.assemble_stiffness_matrix(true).unwrap();
            }
            assert_eq!(
                m.assembly_scratch_footprint(),
                filled,
                "{what}: after the release"
            );
        }
    }
}
