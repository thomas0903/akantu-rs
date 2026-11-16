// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the thread-count invariant of the execution policy (D-123) on the
// staggered solid / phase-field step of
// examples/c++/phase_field_model/phase_field_notch.cc.

//! In [`akantu_core::exec::Precision::Bitwise`] the staggered static step
//! of the solid and phase-field models gives the same bits for every thread
//! count: the solid model's phase-field law (stress, tangent, thermal
//! pre-step on threads), the phase-field model's FE kernels (on its
//! engine's threads), the strain and damage transfers. The displacement,
//! the damage, the solid law's internals and the phase-field internals are
//! compared bit for bit with the one-thread run at 1, 2, 3, 4 and 8
//! threads, finest and automatic split.

use akantu_core::enums::AnalysisMethod;
use akantu_core::exec::{ExecPolicy, Grain};
use akantu_core::{ElementType, GhostType, Real};
use akantu_couplers::CouplerSolidPhaseField;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ConstitutiveLaw, InternalFieldRef};
use akantu_parser::Parser;

const DAT: &str = "model solid_mechanics_model [\n  material phasefield [\n    name = body\n    \
    rho = 1.\n    E = 210.0\n    nu = 0.3\n    eta = 0\n    Plane_Stress = false\n  ]\n]\n\n\
    model phase_field_model [\n  phasefield quadratic [\n    name = body\n    l0 = 0.1\n    \
    gc = 2.7e-3\n    E = 210.0\n    nu = 0.3\n  ]\n]\n";

/// An `n × n` grid of the unit square split into triangles (slightly
/// irregular inside).
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
    m.compute_bounding_box().unwrap();
    m
}

type State = Vec<(String, Vec<u64>)>;

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// The `Real` internals of a law, current values, on triangles.
fn law_bits(prefix: &str, law: &dyn ConstitutiveLaw, out: &mut State) {
    law.for_each_internal(&mut |name, f| {
        if let InternalFieldRef::Real(f) = f
            && f.exists(ElementType::Triangle3, GhostType::NotGhost)
        {
            out.push((
                format!("{prefix}/{name}"),
                bits(
                    f.get(ElementType::Triangle3, GhostType::NotGhost)
                        .unwrap()
                        .as_slice(),
                ),
            ));
        }
    });
}

/// Three load steps of two staggered iterations: the bottom clamped, the
/// top pulled up.
fn run(policy: ExecPolicy) -> State {
    let parser = Parser::parse_str(DAT, "material.dat").unwrap();
    let mut c = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(grid(8)),
        None,
        "coupler_solid_phasefield",
        parser,
    )
    .unwrap();
    c.solid.set_exec_policy(policy).unwrap();
    c.phase.set_exec_policy(policy).unwrap();
    c.solid
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    c.phase
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let y: Vec<Real> = {
        let mesh = c.solid.physics.base.mesh.read().unwrap();
        mesh.nodes()
            .as_slice()
            .chunks_exact(2)
            .map(|p| p[1])
            .collect()
    };
    {
        let mut b = c.solid.blocked_dofs_mut().unwrap();
        let b = b.as_mut_slice();
        for (n, &yn) in y.iter().enumerate() {
            if yn < 1e-9 || yn > 1.0 - 1e-9 {
                b[2 * n] = true;
                b[2 * n + 1] = true;
            }
        }
    }
    for step in 1..=3 {
        {
            let mut u = c.solid.displacement_mut().unwrap();
            let u = u.as_mut_slice();
            for (n, &yn) in y.iter().enumerate() {
                if yn > 1.0 - 1e-9 {
                    u[2 * n + 1] = 0.02 * step as Real;
                }
            }
        }
        for _ in 0..2 {
            c.solid.solve_step(None).unwrap();
            c.compute_strain_on_quad_points(GhostType::NotGhost)
                .unwrap();
            c.phase.solve_step(None).unwrap();
            c.compute_damage_on_quad_points(GhostType::NotGhost)
                .unwrap();
            c.compute_damage_on_quad_points(GhostType::Ghost).unwrap();
            c.solid.assemble_internal_forces().unwrap();
        }
    }
    let mut out = vec![
        ("u".into(), bits(c.solid.displacement().unwrap().as_slice())),
        ("damage".into(), bits(c.phase.damage().unwrap().as_slice())),
        (
            "f_int".into(),
            bits(c.solid.internal_force().unwrap().as_slice()),
        ),
    ];
    for (i, law) in c.solid.physics.materials.laws().iter().enumerate() {
        law_bits(&format!("solid law {i}"), law.as_ref(), &mut out);
    }
    for (i, law) in c.phase.physics.phasefields.laws().iter().enumerate() {
        law_bits(&format!("phase law {i}"), law.as_ref(), &mut out);
    }
    out
}

#[test]
fn staggered_phase_field_step_is_bitwise_for_every_thread_count() {
    let reference = run(ExecPolicy::serial());
    let damaged = reference
        .iter()
        .find(|(n, _)| n == "damage")
        .is_some_and(|(_, v)| v.iter().any(|&b| b != 0));
    assert!(damaged, "the damage moved");
    for threads in [1, 2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            let p = ExecPolicy::serial()
                .with_threads(threads)
                .unwrap()
                .with_grain(grain);
            let what = format!("threads={threads} {grain:?}");
            let s = run(p);
            assert_eq!(reference.len(), s.len(), "{what}");
            for ((n, r), (_, o)) in reference.iter().zip(&s) {
                assert!(!r.is_empty(), "{what}: {n} empty");
                assert!(r == o, "{what}: {n} differs from the one-thread run");
            }
        }
    }
}
