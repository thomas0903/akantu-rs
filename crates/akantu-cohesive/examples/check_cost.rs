// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Measurement of the
// per-step cost of `check_cohesive_stress` (a performance design gate of
// the cohesive model) on a 3-D tetrahedral cube: extrinsic cohesive model, every
// interior facet checked, a uniform tensile strain ramp; prints the time
// split (stress interpolation / law checks / insertion) per step.
//
// Usage: cargo run --release -p akantu-cohesive --example check_cost -- <cube.msh> [steps]

use std::time::Instant;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;

const DAT: &str = "model solid_mechanics_model_cohesive [
  material elastic [
    name = body
    rho = 2500
    E   = 70e9
    nu  = 0.22
  ]
  material cohesive_linear [
    name = insertion
    beta = 3
    G_c = 7.6
    sigma_c = 70e6
    delta_c = 0.217e-6
  ]
]
";

fn main() -> akantu_core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: check_cost <cube.msh> [steps]");
    let steps: usize = args.get(2).map_or(200, |s| s.parse().unwrap_or(200));
    let mut mesh = Mesh::new(3, "cube")?;
    let t = Instant::now();
    akantu_io::read_mesh(&mut mesh, std::path::Path::new(path), MeshIoType::Gmsh)?;
    let (nn, ne) = (
        mesh.nb_nodes(),
        mesh.nb_element(
            akantu_core::ElementType::Tetrahedron4,
            akantu_core::GhostType::NotGhost,
        ),
    );
    let shared = SharedMesh::new(mesh);
    let mut model = SolidMechanicsModelCohesive::with_parser(
        shared.clone(),
        None,
        "smmc",
        Parser::parse_str(DAT, "m.dat")?,
    )?;
    model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)?;
    let t_init = t.elapsed().as_secs_f64();
    let dt = model.stable_time_step()? * 0.1;
    model.set_time_step(dt, None)?;
    // uniform strain rate in z: ε̇ such that σ = σc after `steps` / 2 steps
    let rate = 70e6 / 70e9 / (steps as f64 / 2.0) / dt;
    let x = shared.read()?.nodes().as_slice().to_vec();
    {
        let mut v_g = model.smm.velocity_mut()?;
        let v = v_g.as_mut_slice();
        for n in 0..x.len() / 3 {
            v[3 * n + 2] = rate * x[3 * n + 2];
        }
    }
    let mut t_solve = 0.0;
    let mut inserted_per_step = Vec::new();
    for _ in 0..steps {
        let n = model.check_cohesive_stress()?;
        inserted_per_step.push(n);
        let t = Instant::now();
        model.solve_step(None)?;
        t_solve += t.elapsed().as_secs_f64();
    }
    let s = &model.ext.stats;
    let nf: usize = {
        let m = shared.read()?;
        let fm = m.mesh_facets().expect("facets");
        fm.nb_element(
            akantu_core::ElementType::Triangle3,
            akantu_core::GhostType::NotGhost,
        )
    };
    println!("mesh: {nn} nodes, {ne} tets, {nf} facets (after the run); init_full {t_init:.3} s");
    println!(
        "{} steps: check_cohesive_stress {:.3} ms/step (interpolation {:.3}, checks {:.3}, insertion {:.3}); solve_step {:.3} ms/step; inserted {}",
        s.calls,
        1e3 * (s.interpolation + s.checks + s.insertion) / s.calls as f64,
        1e3 * s.interpolation / s.calls as f64,
        1e3 * s.checks / s.calls as f64,
        1e3 * s.insertion / s.calls as f64,
        1e3 * t_solve / steps as f64,
        s.inserted
    );
    let passes = inserted_per_step.iter().filter(|&&n| n > 0).count();
    if passes > 0 {
        println!(
            "insertion passes: {passes}, mean {:.3} ms per pass (incl. dispatch to the model)",
            1e3 * s.insertion / passes as f64
        );
    }
    Ok(())
}
