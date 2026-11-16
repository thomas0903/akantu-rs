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
//! ([`Grain::Finest`]) so that every threaded kernel really splits, and at
//! the automatic grain. The wider cases add Dirichlet conditions with
//! non-zero prescribed values, a second law that stays serial
//! (`neohookean`, finite deformation) on a third of the elements, an
//! eigen-gradient, 2-D meshes of triangles and quadrangles, and six-node
//! triangles.

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
    let mut s = state(&m);
    // energies: the accumulated external work (a reduction over nodes in
    // the step), kinetic and potential energy
    let energies = ["external work new", "kinetic", "potential", "external work"]
        .map(|id| m.energy(id).unwrap().to_bits())
        .to_vec();
    s.push(("energies".into(), energies));
    s
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

// ---------------------------------------------------------------------------
// Wider models: Dirichlet conditions with prescribed values, a second law
// that stays serial (`neohookean`, `independent_points() = false`, finite
// deformation) on part of the mesh, an eigen-gradient, 2-D meshes of
// triangles and quadrangles, a quadratic element.
// ---------------------------------------------------------------------------

/// An `n × n` grid of the unit square: linear (cells with `i + j` even as
/// quadrangles, the others as two triangles) or quadratic (six-node
/// triangles, two per cell).
fn grid2d(n: usize, quadratic: bool) -> Mesh {
    let mut m = Mesh::new(2, "square").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        let h = 1.0 / n as Real;
        let stride = if quadratic { 2 * n + 1 } else { n + 1 };
        let hh = if quadratic { h / 2.0 } else { h };
        for j in 0..stride {
            for i in 0..stride {
                let inner = i != 0 && j != 0 && i != stride - 1 && j != stride - 1;
                let w = if inner {
                    0.04 * hh * (((i * 7 + j * 3) % 11) as Real / 11.0 - 0.5)
                } else {
                    0.0
                };
                a.push_node(&[i as Real * hh + w, j as Real * hh - w])
                    .unwrap();
            }
        }
        let id = |i: usize, j: usize| (j * stride + i) as i32;
        for j in 0..n {
            for i in 0..n {
                if quadratic {
                    let (x, y) = (2 * i, 2 * j);
                    // corners then mid-edge nodes, counter-clockwise
                    let t1 = [
                        id(x, y),
                        id(x + 2, y),
                        id(x + 2, y + 2),
                        id(x + 1, y),
                        id(x + 2, y + 1),
                        id(x + 1, y + 1),
                    ];
                    let t2 = [
                        id(x, y),
                        id(x + 2, y + 2),
                        id(x, y + 2),
                        id(x + 1, y + 1),
                        id(x + 1, y + 2),
                        id(x, y + 1),
                    ];
                    for t in [t1, t2] {
                        a.connectivity_mut(ElementType::Triangle6, GhostType::NotGhost)
                            .push_tuple(&t);
                    }
                } else if (i + j) % 2 == 0 {
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

/// Elements whose index is a multiple of three take law 1 (`neohookean`),
/// the others law 0 (`elastic`).
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

/// The model of a wider case: two laws (elastic with an eigen-gradient on
/// two thirds of the elements, neohookean on the others), the left side
/// clamped at a non-zero prescribed displacement, the right side's second
/// component prescribed, and an initial field elsewhere.
fn wide_model(mesh: Mesh, policy: ExecPolicy) -> SolidMechanicsModel {
    let dim = mesh.spatial_dimension();
    let eigen = if dim == 2 {
        "[[1e-4, 0], [5e-5, -1e-4]]"
    } else {
        "[[1e-4, 0, 0], [5e-5, -1e-4, 0], [0, 2e-5, 3e-5]]"
    };
    let dat = format!(
        "material elastic [\n  name = a\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
         eigen_grad_u = {eigen}\n]\n\
         material neohookean [\n  name = b\n  rho = 2000\n  E = 50e9\n  nu = 0.3\n]\n"
    );
    let parser = Parser::parse_str(&dat, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    m.set_exec_policy(policy).unwrap();
    m.set_material_selector(Box::new(EveryThird));
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    assert_eq!(m.physics.materials.nb_laws(), 2);
    let dt = m.stable_time_step().unwrap() * 0.5;
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
    let nodes = x.len() / dim;
    let mut prescribed = vec![false; nodes * dim];
    {
        let mut u = m.displacement_mut().unwrap();
        let u = u.as_mut_slice();
        for n in 0..nodes {
            let p = &x[n * dim..(n + 1) * dim];
            for c in 0..dim {
                u[n * dim + c] = 1e-3 * ((5.0 + c as Real) * p[0]).sin() * (p[1] + 0.2);
            }
            if p[0] < 1e-9 {
                // clamped at a non-zero value
                for c in 0..dim {
                    u[n * dim + c] = 2e-4 * (c as Real + 1.0) * (p[1] - 0.5);
                    prescribed[n * dim + c] = true;
                }
            } else if p[0] > 1.0 - 1e-9 {
                u[n * dim + 1] = -3e-4 * p[1];
                prescribed[n * dim + 1] = true;
            }
        }
    }
    m.blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&prescribed);
    m
}

/// Every nodal array and, per law and element type of its filter, the
/// gradient and the stress, plus the energies, after `steps` steps.
fn wide_state(mesh: Mesh, policy: ExecPolicy, steps: usize) -> Vec<(String, Vec<u64>)> {
    let mut m = wide_model(mesh, policy);
    for _ in 0..steps {
        m.solve_step(None).unwrap();
    }
    let bits = |v: &[Real]| v.iter().map(|x| x.to_bits()).collect::<Vec<u64>>();
    let mut out = vec![
        ("u".into(), bits(m.displacement().unwrap().as_slice())),
        ("v".into(), bits(m.velocity().unwrap().as_slice())),
        ("a".into(), bits(m.acceleration().unwrap().as_slice())),
        ("f_int".into(), bits(m.internal_force().unwrap().as_slice())),
    ];
    for (i, law) in m.physics.materials.laws().iter().enumerate() {
        let b = law.base();
        let types = b.filter_types(GhostType::NotGhost);
        assert!(!types.is_empty(), "law {i} has elements");
        for ty in types {
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
    let energies = ["external work new", "kinetic", "potential", "external work"]
        .map(|id| m.energy(id).unwrap().to_bits())
        .to_vec();
    out.push(("energies".into(), energies));
    out
}

#[test]
fn wider_models_are_bitwise_for_every_thread_count() {
    type Case = (&'static str, fn() -> Mesh);
    let cases: [Case; 3] = [
        ("2-D triangles and quadrangles", || grid2d(8, false)),
        ("2-D six-node triangles", || grid2d(6, true)),
        ("3-D tetrahedra and hexahedra", || mixed_cube(3)),
    ];
    for (name, mesh) in cases {
        let serial = ExecPolicy::serial();
        let reference = wide_state(mesh(), serial, 6);
        assert!(
            reference[0].1.iter().any(|&b| b != 0),
            "{name}: the displacement is not zero"
        );
        for threads in [2, 3, 4, 8] {
            for grain in [Grain::Finest, Grain::Auto] {
                let p = serial.with_threads(threads).unwrap().with_grain(grain);
                compare(
                    &reference,
                    &wide_state(mesh(), p, 6),
                    &format!("{name}: threads={threads} {grain:?}"),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Laws whose points are independent (`Material::independent_points`): every
// nodal array, every internal field of the law (current and previous
// values), the energies, bit for bit with the one-thread run at 1, 2, 3, 4
// and 8 threads, finest and automatic grain, in 2-D and 3-D.
// ---------------------------------------------------------------------------

type State = Vec<(String, Vec<u64>)>;

/// One law on the whole mesh: the left side clamped at a non-zero
/// prescribed displacement, the right side's second component prescribed,
/// an initial field of amplitude `amp` elsewhere.
fn law_model(mesh: Mesh, dat: &str, amp: Real, policy: ExecPolicy) -> SolidMechanicsModel {
    let dim = mesh.spatial_dimension();
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    m.set_exec_policy(policy).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let dt = m.stable_time_step().unwrap() * 0.5;
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
    let nodes = x.len() / dim;
    let mut prescribed = vec![false; nodes * dim];
    {
        let mut u = m.displacement_mut().unwrap();
        let u = u.as_mut_slice();
        for n in 0..nodes {
            let p = &x[n * dim..(n + 1) * dim];
            for c in 0..dim {
                u[n * dim + c] = amp * ((5.0 + c as Real) * p[0]).sin() * (p[1] + 0.2);
            }
            if p[0] < 1e-9 {
                for c in 0..dim {
                    u[n * dim + c] = 0.2 * amp * (c as Real + 1.0) * (p[1] - 0.5);
                    prescribed[n * dim + c] = true;
                }
            } else if p[0] > 1.0 - 1e-9 {
                u[n * dim + 1] = -0.3 * amp * p[1];
                prescribed[n * dim + 1] = true;
            }
        }
    }
    m.blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&prescribed);
    m
}

/// The values of every block of `field` on the not-ghost types of
/// `types`, then its previous values when it keeps a history.
fn field_bits<T: Copy>(
    name: &str,
    field: &akantu_model::InternalField<T>,
    types: &[ElementType],
    to_bits: impl Fn(T) -> u64,
    out: &mut State,
) {
    for &ty in types {
        if !field.exists(ty, GhostType::NotGhost) {
            continue;
        }
        let cur = field.get(ty, GhostType::NotGhost).unwrap();
        out.push((
            format!("{name}/{ty:?}"),
            cur.as_slice().iter().map(|&v| to_bits(v)).collect(),
        ));
        drop(cur);
        if field.has_history() {
            let prev = field.previous(ty, GhostType::NotGhost).unwrap();
            out.push((
                format!("{name}/{ty:?}/previous"),
                prev.as_slice().iter().map(|&v| to_bits(v)).collect(),
            ));
        }
    }
}

/// Every nodal array, every internal field of every law, the energies.
fn law_state(m: &mut SolidMechanicsModel) -> State {
    use akantu_model::constitutive_law::InternalFieldRef;
    let bits = |v: &[Real]| v.iter().map(|x| x.to_bits()).collect::<Vec<u64>>();
    let mut out = vec![
        ("u".into(), bits(m.displacement().unwrap().as_slice())),
        ("v".into(), bits(m.velocity().unwrap().as_slice())),
        ("a".into(), bits(m.acceleration().unwrap().as_slice())),
        ("f_int".into(), bits(m.internal_force().unwrap().as_slice())),
    ];
    for (i, law) in m.physics.materials.laws().iter().enumerate() {
        let types = law.base().filter_types(GhostType::NotGhost);
        assert!(!types.is_empty(), "law {i} has elements");
        law.for_each_internal(&mut |name, f| {
            let name = format!("law{i}/{name}");
            match f {
                InternalFieldRef::Real(f) => field_bits(&name, f, &types, Real::to_bits, &mut out),
                InternalFieldRef::Int(f) => field_bits(&name, f, &types, |v| v as u64, &mut out),
                InternalFieldRef::UInt(f) => {
                    field_bits(&name, f, &types, |v| v as u64, &mut out);
                }
                InternalFieldRef::Bool(f) => {
                    field_bits(&name, f, &types, u64::from, &mut out);
                }
            }
        });
    }
    // an energy a law does not define is recorded as missing (the same for
    // every thread count)
    let energies = [
        "kinetic",
        "potential",
        "dissipated",
        "plastic",
        "external work",
    ]
    .map(|id| m.energy(id).map_or(u64::MAX, Real::to_bits))
    .to_vec();
    out.push(("energies".into(), energies));
    out
}

/// A law case: the `.dat` text per dimension, the amplitude of the initial
/// field, the steps, and the internal that must have left its initial value
/// (the law's non-linear branch was reached).
struct LawCase {
    name: &'static str,
    dat: fn(usize) -> String,
    amp: Real,
    steps: usize,
    active: &'static str,
}

fn run_law(case: &LawCase, mesh: Mesh, policy: ExecPolicy) -> State {
    let dim = mesh.spatial_dimension();
    let mut m = law_model(mesh, &(case.dat)(dim), case.amp, policy);
    for _ in 0..case.steps {
        m.solve_step(None).unwrap();
    }
    law_state(&mut m)
}

/// The thread counts and grains of the invariance tests.
fn threaded_policies() -> Vec<ExecPolicy> {
    let serial = ExecPolicy::serial();
    let mut v = Vec::new();
    for threads in [1, 2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            v.push(serial.with_threads(threads).unwrap().with_grain(grain));
        }
    }
    v
}

fn check_law(case: &LawCase) {
    type MeshFn = fn() -> Mesh;
    let meshes: [(&str, MeshFn); 2] = [
        ("2-D triangles and quadrangles", || grid2d(6, false)),
        ("3-D tetrahedra and hexahedra", || mixed_cube(3)),
    ];
    for (mesh_name, mesh) in meshes {
        let reference = run_law(case, mesh(), ExecPolicy::serial());
        let moved = reference
            .iter()
            .filter(|(n, _)| n.contains(&format!("/{}/", case.active)))
            .any(|(_, v)| v.iter().any(|&b| b != 0 && b != 1.0f64.to_bits()));
        assert!(
            moved,
            "{} ({mesh_name}): `{}` never left its initial value",
            case.name, case.active
        );
        for p in threaded_policies() {
            compare(
                &reference,
                &run_law(case, mesh(), p),
                &format!(
                    "{} ({mesh_name}): threads={} {:?}",
                    case.name,
                    p.threads(),
                    p.grain()
                ),
            );
        }
    }
}

fn orthotropic_dat(dim: usize) -> String {
    let axes = if dim == 2 {
        "  n1 = [0.6, 0.8]\n  n2 = [-0.8, 0.6]\n  E1 = 140e9\n  E2 = 10e9\n  nu12 = 0.3\n  \
         G12 = 5e9\n"
    } else {
        "  n1 = [0.6, 0.8, 0]\n  n2 = [-0.8, 0.6, 0]\n  n3 = [0, 0, 1]\n  E1 = 140e9\n  \
         E2 = 10e9\n  E3 = 10e9\n  nu12 = 0.3\n  nu13 = 0.3\n  nu23 = 0.4\n  G12 = 5e9\n  \
         G13 = 5e9\n  G23 = 3.5e9\n"
    };
    format!("material elastic_orthotropic [\n  name = body\n  rho = 2500\n{axes}]\n")
}

#[test]
fn orthotropic_law_is_bitwise_for_every_thread_count() {
    check_law(&LawCase {
        name: "elastic_orthotropic",
        dat: orthotropic_dat,
        amp: 1e-3,
        steps: 6,
        active: "stress",
    });
}

fn anisotropic_dat(dim: usize) -> String {
    let c = if dim == 2 {
        "  n1 = [0.6, 0.8]\n  n2 = [-0.8, 0.6]\n  C11 = 90e9\n  C12 = 30e9\n  C13 = 4e9\n  \
         C22 = 60e9\n  C23 = -3e9\n  C33 = 20e9\n"
    } else {
        "  n1 = [0.6, 0.8, 0]\n  n2 = [-0.8, 0.6, 0]\n  n3 = [0, 0, 1]\n  C11 = 90e9\n  \
         C12 = 30e9\n  C13 = 25e9\n  C14 = 3e9\n  C22 = 70e9\n  C23 = 28e9\n  C33 = 80e9\n  \
         C44 = 20e9\n  C55 = 22e9\n  C56 = -2e9\n  C66 = 24e9\n"
    };
    format!("material elastic_anisotropic [\n  name = body\n  rho = 2500\n{c}]\n")
}

#[test]
fn anisotropic_law_is_bitwise_for_every_thread_count() {
    check_law(&LawCase {
        name: "elastic_anisotropic",
        dat: anisotropic_dat,
        amp: 1e-3,
        steps: 6,
        active: "stress",
    });
}

#[test]
fn maxwell_law_is_bitwise_for_every_thread_count() {
    check_law(&LawCase {
        name: "viscoelastic_maxwell",
        dat: |_| {
            "material viscoelastic_maxwell [\n  name = body\n  rho = 2500\n  Einf = 70e9\n  \
             nu = 0.22\n  Eta = [1e3, 1e4]\n  Ev = [20e9, 10e9]\n]\n"
                .into()
        },
        amp: 1e-3,
        steps: 6,
        active: "sigma_v",
    });
}

#[test]
fn marigo_law_is_bitwise_for_every_thread_count() {
    check_law(&LawCase {
        name: "marigo",
        dat: |_| {
            "material marigo [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
             Yd = 1e3\n  Sd = 1e5\n]\n"
                .into()
        },
        amp: 1e-3,
        steps: 6,
        active: "damage",
    });
}

/// The error path: after `case.steps` steps the displacement of the node
/// nearest the centre becomes NaN, so the points of the elements around it
/// (in the middle of the blocks) fail; the next step's error and the state
/// it leaves are compared bit for bit across thread counts.
fn law_error_state(case: &LawCase, mesh: Mesh, policy: ExecPolicy) -> (String, State) {
    let dim = mesh.spatial_dimension();
    let mut m = law_model(mesh, &(case.dat)(dim), case.amp, policy);
    for _ in 0..case.steps {
        m.solve_step(None).unwrap();
    }
    let centre = {
        let mesh = m.physics.base.mesh.read().unwrap();
        let nodes = mesh.nodes();
        let x = nodes.as_slice();
        let d2 = |n: usize| {
            (0..dim)
                .map(|c| (x[n * dim + c] - 0.5).powi(2))
                .sum::<Real>()
        };
        (0..x.len() / dim)
            .min_by(|&a, &b| d2(a).total_cmp(&d2(b)))
            .unwrap()
    };
    m.displacement_mut().unwrap().as_mut_slice()[centre * dim] = Real::NAN;
    let err = m
        .solve_step(None)
        .expect_err("a NaN displacement makes the law fail")
        .to_string();
    (err, law_state(&mut m))
}

/// `expect`: a part of the error message of the law's failing point.
fn check_law_error(case: &LawCase, expect: &str) {
    type MeshFn = fn() -> Mesh;
    let meshes: [(&str, MeshFn); 2] = [
        ("2-D triangles and quadrangles", || grid2d(6, false)),
        ("3-D tetrahedra and hexahedra", || mixed_cube(3)),
    ];
    for (mesh_name, mesh) in meshes {
        let (err, reference) = law_error_state(case, mesh(), ExecPolicy::serial());
        assert!(
            err.contains(expect),
            "{} ({mesh_name}): the step failed elsewhere: {err}",
            case.name
        );
        for p in threaded_policies() {
            let what = format!(
                "{} error path ({mesh_name}): threads={} {:?}",
                case.name,
                p.threads(),
                p.grain()
            );
            let (e, state) = law_error_state(case, mesh(), p);
            assert_eq!(err, e, "{what}: another error");
            compare(&reference, &state, &what);
        }
    }
}

fn mazars_dat(_: usize) -> String {
    "material mazars [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  K0 = 1e-4\n  \
     At = 1.15\n  Bt = 10000\n  Ac = 0.8\n  Bc = 1391.3\n  beta = 1.0\n]\n"
        .into()
}

const MAZARS: LawCase = LawCase {
    name: "mazars",
    dat: mazars_dat,
    amp: 1e-3,
    steps: 6,
    active: "damage",
};

#[test]
fn mazars_law_is_bitwise_for_every_thread_count() {
    check_law(&MAZARS);
}

#[test]
fn mazars_error_path_is_bitwise_for_every_thread_count() {
    check_law_error(&MAZARS, "eig: no convergence");
}

const NEOHOOKEAN: LawCase = LawCase {
    name: "neohookean",
    dat: |_| {
        "material neohookean [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.3\n]\n".into()
    },
    amp: 2e-2,
    steps: 6,
    active: "grad_u",
};

/// Plane stress in 2-D (the `C33` solve before the stress), plain in 3-D.
const NEOHOOKEAN_PLANE_STRESS: LawCase = LawCase {
    name: "neohookean, plane stress",
    dat: |dim| {
        let ps = if dim == 2 {
            "  Plane_Stress = true\n"
        } else {
            ""
        };
        format!(
            "material neohookean [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.3\n{ps}]\n"
        )
    },
    amp: 2e-2,
    steps: 6,
    active: "grad_u",
};

#[test]
fn neohookean_law_is_bitwise_for_every_thread_count() {
    check_law(&NEOHOOKEAN);
    check_law(&NEOHOOKEAN_PLANE_STRESS);
}

#[test]
fn neohookean_error_path_is_bitwise_for_every_thread_count() {
    check_law_error(&NEOHOOKEAN, "is not positive");
}

/// In 2-D the plane-stress `C33` solve fails first (its points before the
/// failing one keep their new `C33`); in 3-D the stress loop.
#[test]
fn neohookean_plane_stress_error_path_is_bitwise_for_every_thread_count() {
    check_law_error(&NEOHOOKEAN_PLANE_STRESS, "neohookean");
}

#[test]
fn plastic_law_is_bitwise_for_every_thread_count() {
    check_law(&LawCase {
        name: "plastic_linear_isotropic_hardening",
        dat: |_| {
            "material plastic_linear_isotropic_hardening [\n  name = body\n  rho = 2500\n  \
             E = 70e9\n  nu = 0.22\n  h = 5e9\n  sigma_y = 2e7\n]\n"
                .into()
        },
        amp: 1e-3,
        steps: 6,
        active: "inelastic_strain",
    });
}

/// The plastic parent's return and the damage loop, with a thermal strain
/// (the per-block thermal refresh and the thermal work of the energies).
#[test]
fn plastic_mazars_law_is_bitwise_for_every_thread_count() {
    check_law(&LawCase {
        name: "plastic_mazars",
        dat: |_| {
            "material plastic_mazars [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
             h = 5e9\n  sigma_y = 2e7\n  K0 = 1e-4\n  At = 1.15\n  Bt = 10000\n  Ac = 0.8\n  \
             Bc = 1391.3\n  beta = 1.0\n  alpha = 1e-5\n  delta_T = 20\n]\n"
                .into()
        },
        amp: 1e-3,
        steps: 6,
        active: "damage",
    });
}

#[test]
fn marigo_thermal_law_is_bitwise_for_every_thread_count() {
    check_law(&LawCase {
        name: "marigo with a thermal strain",
        dat: |_| {
            "material marigo [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
             Yd = 1e3\n  Sd = 1e5\n  alpha = 1e-5\n  delta_T = 20\n]\n"
                .into()
        },
        amp: 1e-3,
        steps: 6,
        active: "damage",
    });
}

#[test]
fn sls_law_is_bitwise_for_every_thread_count() {
    check_law(&LawCase {
        name: "sls_deviatoric",
        dat: |_| {
            "material sls_deviatoric [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
             Eta = 1e3\n  Ev = 20e9\n]\n"
                .into()
        },
        amp: 1e-3,
        steps: 6,
        active: "history_integral",
    });
}
