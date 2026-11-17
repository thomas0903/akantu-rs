// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the thread-count invariant of the execution policy (D-123) on the
// cohesive model (extrinsic insertion check and insertion, intrinsic
// cohesive elements, the cohesive laws' tractions and residual, energies).

//! In [`Precision::Bitwise`] the cohesive model gives the same bits for
//! every thread count: the nodal state, the facet stresses, every internal
//! of the cohesive laws, the inserted cohesive elements and their order
//! (the cohesive connectivity), and the energies are compared bit for bit
//! with the one-thread run at 1, 2, 3, 4 and 8 threads, with batches split
//! as finely as possible ([`Grain::Finest`], so that every threaded loop
//! really splits) and at the automatic grain. 2-D (triangles) and 3-D
//! (tetrahedra); extrinsic runs with insertions during the run
//! (`cohesive_linear` and `cohesive_bilinear`, the laws that run their
//! points on threads), and intrinsic runs (`cohesive_bilinear`, and
//! `cohesive_exponential`, which stays serial, beside the threaded bulk).

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_cohesive::material::MaterialCohesive;
use akantu_core::enums::AnalysisMethod;
use akantu_core::exec::{ExecPolicy, Grain};
use akantu_core::{ElementKind, ElementType, GhostType, Real, TypeFilter};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;

/// An `n^dim` grid of the unit square (cube), each cell split into two
/// triangles (five tetrahedra); a slightly irregular interior.
fn grid(dim: usize, n: usize) -> Mesh {
    let mut m = Mesh::new(dim, "grid").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        let h = 1.0 / n as Real;
        let nk = if dim == 3 { n } else { 0 };
        let mut xs: Vec<Real> = Vec::new();
        for k in 0..=nk {
            for j in 0..=n {
                for i in 0..=n {
                    let edge = |c: usize, top: usize| c == 0 || c == top;
                    let w = if edge(i, n) || edge(j, n) || (dim == 3 && edge(k, n)) {
                        0.0
                    } else {
                        0.07 * h * (((i * 7 + j * 3 + k * 5) % 11) as Real / 11.0 - 0.5)
                    };
                    let x = [
                        i as Real * h + w,
                        j as Real * h - w,
                        k as Real * h + 0.5 * w,
                    ];
                    a.push_node(&x[..dim]).unwrap();
                    xs.extend_from_slice(&x[..dim]);
                }
            }
        }
        if dim == 2 {
            let id = |i: usize, j: usize| (j * (n + 1) + i) as i32;
            for j in 0..n {
                for i in 0..n {
                    let (a0, a1, a2, a3) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
                    let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
                    c.push_tuple(&[a0, a1, a2]);
                    c.push_tuple(&[a0, a2, a3]);
                }
            }
        } else {
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
                        let mut conn =
                            a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
                        // the six Kuhn tetrahedra around the diagonal 0-6
                        // (conforming between neighbouring cells)
                        for t in [
                            [0, 1, 2, 6],
                            [0, 2, 3, 6],
                            [0, 3, 7, 6],
                            [0, 7, 4, 6],
                            [0, 4, 5, 6],
                            [0, 5, 1, 6],
                        ] {
                            let mut t = t.map(|v| c[v]);
                            orient(&mut t, &xs);
                            conn.push_tuple(&t);
                        }
                    }
                }
            }
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

/// Positive orientation of a tetrahedron (two nodes swapped otherwise).
fn orient(t: &mut [i32; 4], xs: &[Real]) {
    let p = |k: usize| {
        let n = t[k] as usize * 3;
        [xs[n], xs[n + 1], xs[n + 2]]
    };
    let (a, b, c, d) = (p(0), p(1), p(2), p(3));
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let w = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
    let det = u[0] * (v[1] * w[2] - v[2] * w[1]) - u[1] * (v[0] * w[2] - v[2] * w[0])
        + u[2] * (v[0] * w[1] - v[1] * w[0]);
    if det < 0.0 {
        t.swap(2, 3);
    }
}

/// The material input: an elastic bulk and one cohesive law, inserted on
/// the facets whose barycentre lies in the band `y_lo < y < y_hi`.
fn dat(law: &str, y_lo: Real, y_hi: Real) -> String {
    let cohesive = match law {
        "cohesive_linear" => "    sigma_c = 4e5\n    beta = 1\n    G_c = 2\n".to_owned(),
        "cohesive_bilinear" => "    sigma_c = 4e5\n    beta = 1\n    G_c = 2\n    \
                                delta_0 = 2e-7\n    penalty = 1e10\n"
            .to_owned(),
        "cohesive_exponential" => {
            "    sigma_c = 4e5\n    beta = 1\n    delta_c = 1e-5\n".to_owned()
        }
        other => panic!("{other}"),
    };
    format!(
        "model solid_mechanics_model_cohesive [\n  material elastic [\n    name = body\n    \
         rho = 2500\n    E = 7e9\n    nu = 0.22\n  ]\n  material {law} [\n    name = coh\n\
         {cohesive}  ]\n  cohesive_inserter [\n    bounding_box = [[-1, 2], [{y_lo}, {y_hi}], [-1, 2]]\n  \
         ]\n]\n"
    )
}

/// A model of `law` on `grid(dim, n)` with the policy, its time step and an
/// initial stretching velocity (with a shear part) that opens facets
/// during the first steps.
fn model(
    dim: usize,
    n: usize,
    law: &str,
    extrinsic: bool,
    policy: ExecPolicy,
) -> SolidMechanicsModelCohesive {
    // extrinsic: a wide band (many checked facets, insertions spread over
    // the run); intrinsic: the facets of the grid line y = 1/2 only
    let (y_lo, y_hi) = if extrinsic {
        (0.3, 0.7)
    } else {
        let h = 1.0 / n as Real;
        (0.5 - 0.25 * h, 0.5 + 0.25 * h)
    };
    let parser = Parser::parse_str(&dat(law, y_lo, y_hi), "material.dat").unwrap();
    let mut m = SolidMechanicsModelCohesive::with_parser(
        SharedMesh::new(grid(dim, n)),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    if !extrinsic && law == "cohesive_bilinear" {
        // intrinsic bilinear: its strength comes from the parameters
        m.ext.compat.intrinsic_linear_zero_strength = true;
    }
    m.set_exec_policy(policy).unwrap();
    m.init_full(
        &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        extrinsic,
    )
    .unwrap();
    let dt = m.stable_time_step().unwrap() * 0.1;
    m.set_time_step(dt, None).unwrap();
    let x: Vec<Real> = m
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    {
        let mut v = m.smm.velocity_mut().unwrap();
        for (vn, p) in v.as_mut_slice().chunks_mut(dim).zip(x.chunks(dim)) {
            vn[0] = SPEED * (0.2 * (p[0] - 0.5) + 0.04 * p[1]);
            vn[1] = SPEED * (p[1] - 0.5) * (1.0 + 0.2 * p[0]);
            if dim == 3 {
                vn[2] = SPEED * (0.15 * (p[2] - 0.5) - 0.03 * p[0]);
            }
        }
    }
    m
}

/// Amplitude of the initial velocity field.
const SPEED: Real = 1.5;

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// Every state array of the model after the run.
fn state(m: &mut SolidMechanicsModelCohesive) -> Vec<(String, Vec<u64>)> {
    let mut out = vec![
        (
            "u".to_owned(),
            bits(m.smm.displacement().unwrap().as_slice()),
        ),
        ("v".to_owned(), bits(m.smm.velocity().unwrap().as_slice())),
        (
            "a".to_owned(),
            bits(m.smm.acceleration().unwrap().as_slice()),
        ),
        (
            "f_int".to_owned(),
            bits(m.smm.internal_force().unwrap().as_slice()),
        ),
    ];
    for (ty, g, a) in m.ext.facet_stress.as_map().iter_all() {
        out.push((format!("facet_stress/{ty:?}/{g:?}"), bits(a.as_slice())));
    }
    {
        // the cohesive elements (inserted ones in insertion order)
        let mesh = m.smm.physics.base.mesh.read().unwrap();
        let dim = mesh.spatial_dimension();
        for ty in mesh.element_types(TypeFilter {
            spatial_dimension: Some(dim),
            ghost_type: GhostType::NotGhost,
            kind: Some(ElementKind::Cohesive),
        }) {
            let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
            out.push((
                format!("connectivity/{ty:?}"),
                conn.as_slice().iter().map(|&c| c as u64).collect(),
            ));
        }
        out.push(("nodes".to_owned(), bits(mesh.nodes().as_slice())));
    }
    for (i, law) in m.smm.physics.materials.laws().iter().enumerate() {
        let Some(c) = law
            .as_any()
            .and_then(|a| a.downcast_ref::<MaterialCohesive>())
        else {
            continue;
        };
        let core = &c.core;
        let mut fields = vec![
            ("opening", &core.opening),
            ("tractions", &core.tractions),
            ("contact_tractions", &core.contact_tractions),
            ("contact_opening", &core.contact_opening),
            ("delta_max", &core.delta_max),
            ("damage", &core.damage),
            ("normals", &core.normals),
            ("reversible_energy", &core.reversible_energy),
            ("total_energy", &core.total_energy),
        ];
        if let Some(l) = c.law.linear() {
            fields.push(("sigma_c_eff", &l.sigma_c_eff));
            fields.push(("delta_c_eff", &l.delta_c_eff));
            fields.push(("insertion_stress", &l.insertion_stress));
        }
        for (name, f) in fields {
            for (ty, g, a) in f.values().as_map().iter_all() {
                out.push((
                    format!("law{i}/{name}/{ty:?}/{g:?}"),
                    bits(a.borrow().as_slice()),
                ));
            }
        }
    }
    let energies = ["kinetic", "potential", "dissipated", "reversible"]
        .map(|id| m.energy(id).unwrap().to_bits())
        .to_vec();
    out.push(("energies".to_owned(), energies));
    out
}

/// `steps` steps (insertion check, then the solve, for an extrinsic model)
/// and the state; also the number of inserted elements.
fn run(
    dim: usize,
    n: usize,
    law: &str,
    extrinsic: bool,
    policy: ExecPolicy,
    steps: usize,
) -> (Vec<(String, Vec<u64>)>, usize) {
    let mut m = model(dim, n, law, extrinsic, policy);
    let mut inserted = 0;
    for _ in 0..steps {
        if extrinsic {
            inserted += m.check_cohesive_stress().unwrap();
        }
        m.solve_step(None).unwrap();
    }
    if !extrinsic {
        let mesh = m.smm.physics.base.mesh.read().unwrap();
        inserted = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: GhostType::NotGhost,
                kind: Some(ElementKind::Cohesive),
            })
            .map(|ty| mesh.nb_element(ty, GhostType::NotGhost))
            .sum();
    }
    (state(&mut m), inserted)
}

fn compare(reference: &[(String, Vec<u64>)], other: &[(String, Vec<u64>)], what: &str) {
    assert_eq!(reference.len(), other.len(), "{what}: arrays");
    for ((name, r), (o_name, o)) in reference.iter().zip(other) {
        assert_eq!(name, o_name, "{what}");
        assert!(r == o, "{what}: {name} differs from the one-thread run");
    }
}

fn policies() -> Vec<(String, ExecPolicy)> {
    let mut out = Vec::new();
    for threads in [2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            out.push((
                format!("{threads} threads, {grain:?}"),
                ExecPolicy::serial()
                    .with_threads(threads)
                    .unwrap()
                    .with_grain(grain),
            ));
        }
    }
    out
}

fn check(dim: usize, n: usize, law: &str, extrinsic: bool, steps: usize) {
    let (reference, inserted) = run(dim, n, law, extrinsic, ExecPolicy::serial(), steps);
    assert!(
        inserted > 0,
        "{dim}-D {law}: no cohesive element in the run"
    );
    for (what, policy) in policies() {
        let (other, k) = run(dim, n, law, extrinsic, policy, steps);
        assert_eq!(k, inserted, "{dim}-D {law} ({what}): inserted elements");
        compare(
            &reference,
            &other,
            &format!("{dim}-D {law} extrinsic={extrinsic} ({what})"),
        );
    }
}

#[test]
fn extrinsic_linear_is_bitwise_for_every_thread_count() {
    check(2, 10, "cohesive_linear", true, 40);
    check(3, 4, "cohesive_linear", true, 30);
}

#[test]
fn extrinsic_bilinear_is_bitwise_for_every_thread_count() {
    check(2, 10, "cohesive_bilinear", true, 40);
    check(3, 4, "cohesive_bilinear", true, 30);
}

#[test]
fn intrinsic_laws_are_bitwise_for_every_thread_count() {
    check(2, 8, "cohesive_bilinear", false, 30);
    check(3, 3, "cohesive_bilinear", false, 20);
    check(2, 8, "cohesive_exponential", false, 30);
}
