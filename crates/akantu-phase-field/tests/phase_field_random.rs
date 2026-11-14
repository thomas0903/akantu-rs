// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Checks of the
// random critical energy release rate (`gc = base distribution [..]`, a
// RandomInternalField drawn from the process-global generator, D-006) and
// of the upstream defect UD-379 (per-element energy with the first point's
// g_c). The tests of this binary draw from the global engine, so they run
// one at a time (`SERIAL`).

mod common;

use std::sync::Mutex;

use akantu_core::random::{IdxSlot, RandomGenerator};
use akantu_core::{Array, Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_parser::ParamValue;
use akantu_phase_field::{PhaseFieldModel, PhaseFieldUpstreamCompat};
use common::*;

static SERIAL: Mutex<()> = Mutex::new(());

const RANDOM_GC: &str = "phasefield quadratic [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3 uniform [0, 1e-3]\n l0 = 0.5\n]\n";

fn quads() -> Mesh {
    let mut mesh = Mesh::new(2, "quads").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for j in 0..3 {
            for i in 0..3 {
                a.push_node(&[0.5 * i as Real + 0.1 * j as Real, 0.5 * j as Real])
                    .unwrap();
            }
        }
        let mut c = a.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
        let id = |i: usize, j: usize| (i + 3 * j) as Idx;
        for j in 0..2 {
            for i in 0..2 {
                c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
    }
    mesh
}

fn random_model(seed: i64, compat: PhaseFieldUpstreamCompat) -> PhaseFieldModel {
    RandomGenerator::<IdxSlot>::seed(seed);
    let mut model = model_with(quads(), RANDOM_GC);
    model.set_upstream_compat(compat);
    init_static(&mut model);
    model
}

fn gc_values(model: &PhaseFieldModel) -> Vec<Real> {
    model
        .phase_field(0)
        .unwrap()
        .base()
        .g_c
        .get(ElementType::Quadrangle4, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec()
}

/// Draws of `gc = 5e-3 uniform [0, 1e-3]`: one per quadrature point, in
/// `[5e-3, 6e-3)`, not constant, reproducible with the same seed, different
/// with another; the parameter reads back as given and the penalty uses its
/// base value.
#[test]
fn random_gc_draws() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let a = gc_values(&random_model(42, PhaseFieldUpstreamCompat::default()));
    let b = gc_values(&random_model(42, PhaseFieldUpstreamCompat::default()));
    let c = gc_values(&random_model(43, PhaseFieldUpstreamCompat::default()));
    assert_eq!(a.len(), 16, "4 elements × 4 points");
    assert!(a.iter().all(|&g| (5e-3..6e-3).contains(&g)));
    assert!(a.iter().any(|&g| g != a[0]));
    assert_eq!(
        a.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        b.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
    );
    assert_ne!(a, c);
    let model = random_model(42, PhaseFieldUpstreamCompat::default());
    match model.phase_field(0).unwrap().get_param("gc").unwrap() {
        ParamValue::Random(p) => {
            assert_eq!(p.base_value, 5e-3);
            assert!(p.distribution.is_some());
        }
        other => panic!("gc: {other:?}"),
    }
    assert_eq!(model.phase_field(0).unwrap().base().gc(), 5e-3);
}

/// Per-element dissipated energy with a random `g_c` (four points per
/// element): the port integrates each point's own `g_c` (UD-379; upstream
/// keeps the first point's), so the element energies add up to the total.
#[test]
fn ud379_element_energy_uses_each_points_gc() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for compat in [
        PhaseFieldUpstreamCompat::default(),
        PhaseFieldUpstreamCompat::upstream(),
    ] {
        let mut model = random_model(7, compat);
        {
            let mesh_h = model.mesh_handle().clone();
            let mesh = mesh_h.read().unwrap();
            let x = mesh.nodes().as_slice().to_vec();
            let mut d_g = model.damage_mut().unwrap();
            let d = d_g.as_mut_slice();
            for (n, dn) in d.iter_mut().enumerate() {
                *dn = 0.1 + 0.3 * x[2 * n] - 0.2 * x[2 * n + 1];
            }
        }
        let total = model.energy("dissipated").unwrap();
        let gc = gc_values(&model);
        let base = model.phase_field(0).unwrap().base();
        let dq = base
            .damage
            .get(ElementType::Quadrangle4, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .to_vec();
        let gd = base
            .grad_d
            .get(ElementType::Quadrangle4, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .to_vec();
        let l0 = 0.5;
        let mut sum = 0.0;
        for e in 0..4usize {
            let mut density = Vec::new();
            for q in 0..4 {
                let p = 4 * e + q;
                let g = if compat.element_energy_first_point_gc {
                    gc[4 * e]
                } else {
                    gc[p]
                };
                let mut edis = 0.;
                for k in 0..2 {
                    edis += 0.5 * g * l0 * gd[2 * p + k] * gd[2 * p + k];
                }
                edis += g * dq[p] * dq[p] / (2. * l0);
                density.push(edis);
            }
            let mut out = Array::new(1);
            model
                .fe_engine()
                .unwrap()
                .integrate(
                    Array::from_vec(density, 1).unwrap().view(),
                    &mut out,
                    1,
                    ElementType::Quadrangle4,
                    GhostType::NotGhost,
                    Some(&[e as Idx]),
                )
                .unwrap();
            let el = Element::new(ElementType::Quadrangle4, e as Idx, GhostType::NotGhost);
            let got = model.energy_of_element("dissipated", el).unwrap();
            assert_eq!(got, out.as_slice()[0], "compat {compat:?}: element {e}");
            sum += got;
        }
        let gap = rel(sum, total);
        if compat.element_energy_first_point_gc {
            assert!(
                gap > 1e-6,
                "upstream: the element energies miss the g_c field ({gap:e})"
            );
        } else {
            assert!(gap <= 1e-14, "Σ elements {sum} vs total {total}");
        }
    }
}
