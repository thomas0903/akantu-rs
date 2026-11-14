// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Expectations of the
// dumper example tests (tests/examples_dumper_low_level.rs,
// tests/examples_dumpable_interface.rs): the swiss train mesh of
// examples/c++/solid_mechanics_model/io/dumper, its wheels, colours from the
// physical names and the motion of the train.

use std::collections::BTreeMap;
use std::path::Path;

use akantu::core::enums::MeshIoType;
use akantu::core::{ElementType, GhostType};
use akantu::io::read_mesh;
use akantu::mesh::Mesh;

use super::{Vtu, read_vtu};

/// Number of steps of the reduced runs.
pub const STEPS: usize = 3;

/// Expectations on the train mesh.
pub struct Train {
    pub x: Vec<f64>,
    pub wheel_nodes: Vec<usize>,
    pub wheel_elements: Vec<usize>,
    pub wheel_of: BTreeMap<usize, usize>,
    pub colour: Vec<i64>,
    pub centers: [[f64; 2]; 2],
}

/// The train mesh, its wheel group and the colours expected from the
/// physical names (independent of the examples' helpers).
pub fn train() -> Train {
    let mut mesh = Mesh::new(2, "train").unwrap();
    read_mesh(
        &mut mesh,
        &super::frozen_example_mesh("solid_mechanics_model/io/dumper/swiss_train.msh"),
        MeshIoType::Auto,
    )
    .unwrap();
    let wheels = ["lwheel_1", "lwheel_2", "rwheel_1", "rwheel_2"];
    let mut wheel_of = BTreeMap::new();
    for (w, name) in wheels.iter().enumerate() {
        for &n in mesh
            .groups()
            .element_group(name)
            .unwrap()
            .nodes()
            .as_slice()
        {
            wheel_of.insert(n as usize, w);
        }
    }
    mesh.groups_mut().create_element_group("wheels", 2).unwrap();
    for name in wheels {
        mesh.groups_mut()
            .append_element_group("wheels", name)
            .unwrap();
    }
    let g = mesh.groups().element_group("wheels").unwrap();
    let wheel_nodes: Vec<usize> = g.nodes().iter().map(|&n| n as usize).collect();
    let wheel_elements: Vec<usize> = g
        .elements_of(ElementType::Triangle3, GhostType::NotGhost)
        .iter()
        .map(|&e| e as usize)
        .collect();
    let names = mesh
        .data()
        .strings("physical_names")
        .unwrap()
        .get(ElementType::Triangle3, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    let colour = names
        .iter()
        .map(|n| match n.as_str() {
            "red" => 3,
            "white" | "lwheel_1" | "rwheel_1" => 2,
            _ => 1,
        })
        .collect();
    let x = mesh.nodes().as_slice().to_vec();
    let centers = [[x[28], x[29]], [x[4], x[5]]];
    Train {
        x,
        wheel_nodes,
        wheel_elements,
        wheel_of,
        colour,
        centers,
    }
}

/// Displacement of node `n` at step `i`: the translation `50 i/100` in x,
/// plus (unless `compat`) the rotation by `θ_i = −50 i/100` rad of the
/// wheels around their centres `c`. Wheel nodes are checked against the
/// closed form `R(θ_i)(x − c) − (x − c) + (shift, 0)` (rotation matrix,
/// independent of the example's `acos` formulation; a
/// distance-to-centre check also holds when the wheels do not turn); the
/// other nodes are translated exactly. Bound `1e-7 · max(r, 1)`: the
/// example's angle `acos(p_x / r)` is ill-conditioned near `±1`, where the
/// rounding of `p_x / r` moves it by at most `√(2·2ε) ≈ 2.1e-8` rad
/// (`> 4×` margin); a missing rotation misses by `2r |sin(θ_i/2)|`
/// (`≈ 0.49 r` at step 1). Returns the number of wheel nodes checked.
pub fn check_motion(t: &Train, i: usize, u: &[f64], nc: usize, compat: bool) -> usize {
    #[allow(clippy::cast_precision_loss)]
    let shift = i as f64 / 100. * 50.;
    #[allow(clippy::cast_precision_loss)]
    let theta = -(i as f64 / 100. * 50.);
    let (sin, cos) = theta.sin_cos();
    let mut wheel_nodes = 0;
    for n in 0..t.x.len() / 2 {
        let (x, y) = (t.x[2 * n], t.x[2 * n + 1]);
        let (ux, uy) = (u[nc * n], u[nc * n + 1]);
        match t.wheel_of.get(&n) {
            Some(&w) if !compat => {
                let c = t.centers[w / 2];
                let (px, py) = (x - c[0], y - c[1]);
                let r = (px * px + py * py).sqrt();
                if r < 1e-12 {
                    // the centre itself (upstream skips radius < 1e-12)
                    assert_eq!((ux, uy), (shift, 0.0), "centre node {n}");
                    continue;
                }
                let want = [cos * px - sin * py - px + shift, sin * px + cos * py - py];
                let err = (ux - want[0]).abs().max((uy - want[1]).abs());
                assert!(
                    err <= 1e-7 * r.max(1.),
                    "wheel node {n} at step {i}: displacement ({ux}, {uy}), rotated ({}, {}), error {err}",
                    want[0],
                    want[1]
                );
                wheel_nodes += 1;
            }
            _ => {
                assert_eq!((ux, uy), (shift, 0.0), "node {n} at step {i}");
            }
        }
        if nc == 3 {
            assert_eq!(u[nc * n + 2], 0.0);
        }
    }
    wheel_nodes
}

/// The values of `full` (one tuple of `nc` per entity) at the entities
/// `ids`.
pub fn restrict(full: &[f64], nc: usize, ids: &[usize]) -> Vec<f64> {
    ids.iter()
        .flat_map(|&i| full[nc * i..nc * (i + 1)].iter().copied())
        .collect()
}

pub fn vtu(dir: &Path, name: &str, step: usize) -> Vtu {
    read_vtu(&dir.join(format!("{name}-VTU/{name}_{step:04}.vtu")))
}
