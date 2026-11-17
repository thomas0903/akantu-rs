// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the thread-count invariant of the execution policy (D-123) on the laws
// of the extra-materials package that declare independent points.

//! In [`akantu_core::exec::Precision::Bitwise`] an explicit step with
//! `visco_plastic` (corrected and upstream modes), `damage_linear`
//! (per-point thresholds) and `ve_stiffness_prop` gives the same bits for
//! every thread count: the nodal arrays, every `Real` internal field of the
//! law (current and previous values) and the energies are compared bit for
//! bit with the one-thread run at 1, 2, 3, 4 and 8 threads, finest and
//! automatic grain, in 2-D and 3-D.

use akantu_core::array_shared::ForeignRuntime;
use akantu_core::array_storage::ExportLease;
use akantu_core::enums::AnalysisMethod;
use akantu_core::exec::{ExecPolicy, Grain};
use akantu_core::{ElementType, GhostType, Real};
use akantu_extra_materials::{ExtraLawOptions, register_extra_materials_with};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{InternalFieldMut, InternalFieldRef};
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

type State = Vec<(String, Vec<u64>)>;

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// `steps` explicit steps of one law on the whole mesh: the left side
/// clamped at a non-zero value, the right side's second component
/// prescribed, an initial field of amplitude 1e-3 elsewhere.
/// With `copy_every_save` the blocks of every field with history are
/// exported to a foreign runtime (as NumPy views would be), which makes
/// every history save a copy; otherwise the saves are deferred.
/// With `poison`, the displacement of the node nearest the centre then
/// becomes NaN and one more step must fail: its error message is returned
/// with the state it leaves (empty without `poison`).
fn run(
    mesh: Mesh,
    dat: &str,
    options: ExtraLawOptions,
    policy: ExecPolicy,
    copy_every_save: bool,
    poison: bool,
) -> (String, State) {
    let dim = mesh.spatial_dimension();
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    register_extra_materials_with(&mut m.physics.factory, options).unwrap();
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
                u[n * dim + c] = 1e-3 * ((5.0 + c as Real) * p[0]).sin() * (p[1] + 0.2);
            }
            if p[0] < 1e-9 {
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
    let rt = ForeignRuntime::new();
    let mut leases: Vec<ExportLease<Real>> = Vec::new();
    if copy_every_save {
        for law in m.physics.materials.laws_mut() {
            let types = law.base().filter_types(GhostType::NotGhost);
            law.for_each_internal_mut(&mut |_, field| {
                if let InternalFieldMut::Real(f) = field
                    && f.has_history()
                {
                    for &ty in &types {
                        if f.exists(ty, GhostType::NotGhost) {
                            leases.push(
                                f.values_mut()
                                    .shared_mut(ty, GhostType::NotGhost)?
                                    .export(&rt)?,
                            );
                        }
                    }
                }
                Ok(())
            })
            .unwrap();
        }
        assert!(!leases.is_empty(), "fields with history were exported");
    }
    let mut interp = rt.enter().unwrap();
    let _native = interp.native().unwrap();
    for _ in 0..6 {
        m.solve_step(None).unwrap();
    }
    let mut error = String::new();
    if poison {
        let d2 = |n: usize| {
            (0..dim)
                .map(|c| (x[n * dim + c] - 0.5).powi(2))
                .sum::<Real>()
        };
        let centre = (0..nodes).min_by(|&a, &b| d2(a).total_cmp(&d2(b))).unwrap();
        m.displacement_mut().unwrap().as_mut_slice()[centre * dim] = Real::NAN;
        error = m
            .solve_step(None)
            .expect_err("a NaN displacement makes the law fail")
            .to_string();
    }
    let mut out = vec![
        ("u".into(), bits(m.displacement().unwrap().as_slice())),
        ("v".into(), bits(m.velocity().unwrap().as_slice())),
        ("a".into(), bits(m.acceleration().unwrap().as_slice())),
        ("f_int".into(), bits(m.internal_force().unwrap().as_slice())),
    ];
    for (i, law) in m.physics.materials.laws().iter().enumerate() {
        let types = law.base().filter_types(GhostType::NotGhost);
        law.for_each_internal(&mut |name, f| {
            if let InternalFieldRef::Real(f) = f {
                for &ty in &types {
                    if !f.exists(ty, GhostType::NotGhost) {
                        continue;
                    }
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
        });
    }
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
    (error, out)
}

/// `active`: an internal that must leave zero (the law's non-linear branch
/// was reached).
fn check(name: &str, dat: &str, options: ExtraLawOptions, active: &str) {
    type MeshFn = fn() -> Mesh;
    let meshes: [(&str, MeshFn); 2] = [("2-D", || grid2d(6, false)), ("3-D", || mixed_cube(3))];
    for (mesh_name, mesh) in meshes {
        let (_, reference) = run(mesh(), dat, options, ExecPolicy::serial(), false, false);
        let moved = reference
            .iter()
            .filter(|(n, _)| n.contains(&format!("/{active}/")))
            .any(|(_, v)| v.iter().any(|&b| b != 0));
        assert!(moved, "{name} ({mesh_name}): `{active}` stayed zero");
        for threads in [1, 2, 3, 4, 8] {
            for grain in [Grain::Finest, Grain::Auto] {
                let p = ExecPolicy::serial()
                    .with_threads(threads)
                    .unwrap()
                    .with_grain(grain);
                // deferred history saves at every thread count, saves by
                // copy (exported blocks) at 1, 4 and 8 threads
                for copy in [false, true] {
                    if copy && !matches!(threads, 1 | 4 | 8) {
                        continue;
                    }
                    let what =
                        format!("{name} ({mesh_name}): threads={threads} {grain:?} copy={copy}");
                    let (_, s) = run(mesh(), dat, options, p, copy, false);
                    assert_eq!(reference.len(), s.len(), "{what}");
                    for ((n, r), (_, o)) in reference.iter().zip(&s) {
                        assert!(!r.is_empty(), "{what}: {n} empty");
                        assert!(r == o, "{what}: {n} differs from the one-thread run");
                    }
                }
            }
        }
    }
}

/// The error path: after six steps a NaN displacement at the centre node
/// makes the points around it (in the middle of the blocks) fail; the
/// error and every current and previous value equal the serial run's at
/// every thread count, with deferred saves and with saves by copy.
/// `expect`: a part of the error message of the failing point.
fn check_error(name: &str, dat: &str, options: ExtraLawOptions, expect: &str) {
    type MeshFn = fn() -> Mesh;
    let meshes: [(&str, MeshFn); 2] = [("2-D", || grid2d(6, false)), ("3-D", || mixed_cube(3))];
    for (mesh_name, mesh) in meshes {
        let (err, reference) = run(mesh(), dat, options, ExecPolicy::serial(), false, true);
        assert!(
            err.contains(expect),
            "{name} ({mesh_name}): the step failed elsewhere: {err}"
        );
        for threads in [1, 2, 3, 4, 8] {
            for grain in [Grain::Finest, Grain::Auto] {
                let p = ExecPolicy::serial()
                    .with_threads(threads)
                    .unwrap()
                    .with_grain(grain);
                for copy in [false, true] {
                    if copy && !matches!(threads, 1 | 4 | 8) {
                        continue;
                    }
                    let what = format!(
                        "{name} error path ({mesh_name}): threads={threads} {grain:?} copy={copy}"
                    );
                    let (e, s) = run(mesh(), dat, options, p, copy, true);
                    assert_eq!(err, e, "{what}: another error");
                    assert_eq!(reference.len(), s.len(), "{what}");
                    for ((n, r), (_, o)) in reference.iter().zip(&s) {
                        assert!(r == o, "{what}: {n} differs from the one-thread run");
                    }
                }
            }
        }
    }
}

const VISCO: &str = "material visco_plastic [\n  name = body\n  rho = 2500\n  E = 70e9\n  \
    nu = 0.22\n  sigma_y = 2e7\n  h = 5e9\n  rate = 1\n  edot0 = 0.1\n  ts = 1e-6\n]\n";

#[test]
fn visco_plastic_law_is_bitwise_for_every_thread_count() {
    check(
        "visco_plastic",
        VISCO,
        ExtraLawOptions::default(),
        "inelastic_strain",
    );
    check(
        "visco_plastic, upstream mode",
        VISCO,
        ExtraLawOptions::upstream(),
        "inelastic_strain",
    );
}

#[test]
fn damage_linear_law_is_bitwise_for_every_thread_count() {
    check(
        "damage_linear",
        "material damage_linear [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
         Sigc = 3e7\n  Gc = 2\n]\n",
        ExtraLawOptions::default(),
        "damage",
    );
}

#[test]
fn ve_stiffness_prop_law_is_bitwise_for_every_thread_count() {
    check(
        "ve_stiffness_prop",
        "material ve_stiffness_prop [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
         Alpha = 1e-6\n]\n",
        ExtraLawOptions::default(),
        "stress_viscosity",
    );
}

const DAMAGE_LINEAR: &str = "material damage_linear [\n  name = body\n  rho = 2500\n  E = 70e9\n  \
    nu = 0.22\n  Sigc = 3e7\n  Gc = 2\n]\n";

/// `damage_linear` works on copies of its blocks, written back only when
/// every point succeeded: a failed eigen solve (NaN strain) leaves its
/// state as the serial loop leaves it.
#[test]
fn damage_linear_error_path_is_bitwise_for_every_thread_count() {
    check_error(
        "damage_linear",
        DAMAGE_LINEAR,
        ExtraLawOptions::default(),
        "eig",
    );
}
