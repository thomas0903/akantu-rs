// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the fast-mode harness of D-123 run end to end on the fast kernels of the
// explicit step of examples/c++/solid_mechanics_model/explicit.

//! The explicit step in `Precision::Fast` (the gradient, the integrated
//! `BᵀD` and the integrated finite-deformation residual with fused
//! multiply-adds, `FAST MODE` in `akantu_fe` and `akantu_solid`) against
//! `Precision::Bitwise`, through `akantu_validation::fast_mode` and the rule
//! of `fixtures/fast_mode/solid/tolerances.json`; also at several thread
//! counts (the fast kernels' results do not depend on them either).

use akantu_core::enums::AnalysisMethod;
use akantu_core::exec::{ExecPolicy, Grain, Precision};
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fast_mode::{ModeQuantity, check_fast_mode};

const CASE: &str = "fast_mode/solid/explicit_fma_kernels";
const STEPS: usize = 20;

/// An `n³` grid of tetrahedra (six per cell) or hexahedra, slightly
/// irregular.
fn cube(n: usize, hex: bool) -> Mesh {
    let mut m = Mesh::new(3, "cube").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        let h = 1.0 / n as Real;
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    let inner = [i, j, k].iter().all(|&c| c != 0 && c != n);
                    let w = if inner {
                        0.05 * h * (((i * 7 + j * 3 + k * 5) % 11) as Real / 11.0 - 0.5)
                    } else {
                        0.0
                    };
                    a.push_node(&[i as Real * h + w, j as Real * h - w, k as Real * h + w])
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
                    if hex {
                        a.connectivity_mut(ElementType::Hexahedron8, GhostType::NotGhost)
                            .push_tuple(&c);
                        continue;
                    }
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
    m.compute_bounding_box().unwrap();
    m
}

fn run(fd: bool, hex: bool, policy: ExecPolicy) -> Vec<ModeQuantity> {
    let dat = format!(
        "material elastic [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
         finite_deformation = {fd}\n]\n"
    );
    let parser = Parser::parse_str(&dat, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(cube(5, hex)), None, "smm", parser)
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
    let s = if fd { 2e-2 } else { 1e-3 };
    for (u, p) in m
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .as_chunks_mut::<3>()
        .0
        .iter_mut()
        .zip(x.as_chunks::<3>().0)
    {
        u[0] = s * (6.0 * p[0]).sin() * (p[1] + 0.3);
        u[1] = s * (5.0 * p[2]).cos() * p[0];
        u[2] = -s * (4.0 * p[1] * p[2]).sin();
    }
    for _ in 0..STEPS {
        m.solve_step(None).unwrap();
    }
    vec![
        ModeQuantity::nodal("u", 3, m.displacement().unwrap().as_slice().to_vec()),
        ModeQuantity::nodal("v", 3, m.velocity().unwrap().as_slice().to_vec()),
        ModeQuantity::nodal("a", 3, m.acceleration().unwrap().as_slice().to_vec()),
        ModeQuantity::nodal("f_int", 3, m.internal_force().unwrap().as_slice().to_vec()),
    ]
}

#[test]
fn fast_mode_explicit_step_within_its_rule() {
    for (fd, hex) in [(false, false), (true, false), (false, true), (true, true)] {
        let bitwise = run(fd, hex, ExecPolicy::serial());
        let fast_policy = ExecPolicy::serial().with_precision(Precision::Fast);
        let fast = run(fd, hex, fast_policy);
        for r in check_fast_mode(CASE, &bitwise, &fast, true) {
            let w = r.worst.as_ref().map_or(0.0, |w| w.ratio);
            eprintln!(
                "fd={fd} hex={hex} {}: {} entries, {} exact, worst ratio {w:.3e}, max rel {:?}",
                r.quantity,
                r.n_compared,
                r.n_exact,
                r.max_rel_err.as_ref().map(|d| d.rel_err)
            );
        }
        // the fast kernels' results do not depend on the thread count
        for threads in [2, 4] {
            let p = fast_policy
                .with_threads(threads)
                .unwrap()
                .with_grain(Grain::Finest);
            assert_eq!(run(fd, hex, p), fast, "fd={fd} hex={hex} threads={threads}");
        }
    }
}
