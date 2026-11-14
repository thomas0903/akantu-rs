// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Timing harness of
// the extrinsic cohesive explicit step: the material set of the
// cohesive scalability benchmark (elastic bulk with finite deformation,
// cohesive_linear), every interior facet checked, a uniform tensile strain
// ramp that stays below the cohesive strength (no insertion).

//! Extrinsic cohesive step timing (`docs/BENCHMARKS.md`).
//!
//! ```text
//! cargo run --release -p akantu-cohesive --example cohesive_step_timing -- \
//!     --mesh cube.msh [--small-strain] [--steps 20] [--reps 1] [--warmup 3] [--label L]
//! ```
//!
//! Per repetition: `--steps` calls of `check_cohesive_stress` (phase
//! `check`) and of `solve_step` (phase `step`), one untimed call of each
//! first; JSON lines as `akantu-solid`'s `explicit_step_timing`
//! (`ns_el_step` per tetrahedron and step). The velocity ramp
//! `v_z = ε̇·z` reaches 30 % of `σ_c` after all steps of the run, so no
//! cohesive element is inserted (checked: the run fails otherwise).

use std::path::PathBuf;
use std::time::Instant;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{ElementType, Error, GhostType, Result};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;

fn dat(fd: bool) -> String {
    format!(
        "model solid_mechanics_model_cohesive [
  material elastic [
    name = body
    rho = 2500
    E   = 70e9
    nu  = 0.22
    finite_deformation = {fd}
  ]
  material cohesive_linear [
    name = insertion
    beta = 3
    G_c = 7.6
    sigma_c = 70e6
    delta_c = 0.217e-6
  ]
]
"
    )
}

fn usage() -> Error {
    Error::invalid(
        "usage: cohesive_step_timing --mesh FILE [--small-strain] [--steps S] [--reps R] \
         [--warmup W] [--label L]",
    )
}

fn main() -> Result<()> {
    let (mut mesh_path, mut fd, mut steps, mut reps, mut warmup, mut label) =
        (PathBuf::new(), true, 20usize, 1usize, 3usize, String::new());
    let mut it = std::env::args().skip(1);
    let num =
        |v: Option<String>| -> Result<usize> { v.ok_or_else(usage)?.parse().map_err(|_| usage()) };
    while let Some(k) = it.next() {
        match k.as_str() {
            "--mesh" => mesh_path = PathBuf::from(it.next().ok_or_else(usage)?),
            "--small-strain" => fd = false,
            "--steps" => steps = num(it.next())?,
            "--reps" => reps = num(it.next())?,
            "--warmup" => warmup = num(it.next())?,
            "--label" => label = it.next().ok_or_else(usage)?,
            _ => return Err(usage()),
        }
    }
    if mesh_path.as_os_str().is_empty() {
        return Err(usage());
    }
    let t0 = Instant::now();
    let mut mesh = Mesh::new(3, "cube")?;
    akantu_io::read_mesh(&mut mesh, &mesh_path, MeshIoType::Gmsh)?;
    let ne = mesh.nb_element(ElementType::Tetrahedron4, GhostType::NotGhost);
    let shared = SharedMesh::new(mesh);
    let mut model = SolidMechanicsModelCohesive::with_parser(
        shared.clone(),
        None,
        "smmc",
        Parser::parse_str(&dat(fd), "m.dat")?,
    )?;
    model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)?;
    let dt = model.stable_time_step()? * 0.1;
    model.set_time_step(dt, None)?;
    let init_s = t0.elapsed().as_secs_f64();
    let total = warmup + reps * (steps + 1);
    let rate = 0.3 * 70e6 / 70e9 / (total as f64 * dt);
    let x = shared.read()?.nodes().as_slice().to_vec();
    {
        let mut v_g = model.smm.velocity_mut()?;
        let v = v_g.as_mut_slice();
        for n in 0..x.len() / 3 {
            v[3 * n + 2] = rate * x[3 * n + 2];
        }
    }
    println!(
        "{{\"kind\":\"meta\",\"label\":{label:?},\"mesh\":{:?},\"types\":[\"_tetrahedron_4\"],\
         \"nb_elements\":{ne},\"nb_nodes\":{},\"fd\":{fd},\"init_s\":{init_s:.6},\"time_step\":{dt:e}}}",
        mesh_path.display().to_string(),
        x.len() / 3
    );
    let mut inserted = 0;
    for _ in 0..warmup {
        inserted += model.check_cohesive_stress()?;
        model.solve_step(None)?;
    }
    for rep in 0..reps {
        for phase in ["check", "step"] {
            let run = |m: &mut SolidMechanicsModelCohesive| -> Result<usize> {
                if phase == "check" {
                    m.check_cohesive_stress()
                } else {
                    m.solve_step(None).map(|()| 0)
                }
            };
            inserted += run(&mut model)?;
            let t = Instant::now();
            for _ in 0..steps {
                inserted += run(&mut model)?;
            }
            let s = t.elapsed().as_secs_f64();
            let per = s / (ne as f64 * steps as f64) * 1e9;
            println!(
                "{{\"kind\":\"time\",\"label\":{label:?},\"phase\":{phase:?},\"rep\":{rep},\
                 \"steps\":{steps},\"seconds\":{s:.6},\"ns_el_step\":{per:.3}}}"
            );
        }
    }
    if inserted != 0 {
        return Err(Error::invalid(format!(
            "{inserted} cohesive elements inserted: not a pre-insertion measurement"
        )));
    }
    Ok(())
}
