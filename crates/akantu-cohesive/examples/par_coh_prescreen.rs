// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: pre-screens the probe scenarios
// (akantu_cohesive::par_scenarios) on one rank with the serial port, which is
// bitwise with the serial oracle on these laws, so that the oracle run
// inserts and opens within the step budget.

//! `cargo run -p akantu-cohesive --example par_coh_prescreen [scenario…]`:
//! per scenario the first insertion step, the inserted counts and the
//! final energies.

use akantu_cohesive::par_scenarios::{self, NoObserver};
use akantu_mesh::{Mesh, SharedMesh};

fn main() -> akantu_core::Result<()> {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for spec in par_scenarios::specs() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == spec.name) {
            continue;
        }
        let mut mesh = Mesh::new(spec.dim, "mesh")?;
        par_scenarios::build_mesh(&spec, &mut mesh, true)?;
        let shared = SharedMesh::new(mesh);
        let (mut model, dt, recs) = par_scenarios::run(&spec, shared.clone(), &mut NoObserver)?;
        let first = recs.iter().position(|r| r.inserted > 0).map(|p| p + 1);
        let total: usize = recs.iter().map(|r| r.inserted).sum();
        let ins: Vec<(usize, usize)> = recs
            .iter()
            .enumerate()
            .filter(|(_, r)| r.inserted > 0)
            .map(|(i, r)| (i + 1, r.inserted))
            .collect();
        let last = recs.last().map(|r| r.energies).unwrap_or_default();
        let ncoh = {
            let m = shared.read()?;
            m.element_types(akantu_core::TypeFilter {
                spatial_dimension: Some(spec.dim),
                ghost_type: akantu_core::GhostType::NotGhost,
                kind: Some(akantu_core::ElementKind::Cohesive),
            })
            .map(|t| m.nb_element(t, akantu_core::GhostType::NotGhost))
            .sum::<usize>()
        };
        println!(
            "{}: dt {:?}, cohesive elements {ncoh}, first insertion step {first:?}, \
             inserted {total} {ins:?}",
            spec.name, dt
        );
        println!("  final energies (kin, pot, dis, rev, contact): {last:?}");
        for (k, r) in recs.iter().enumerate() {
            if (k + 1) % 10 == 0 {
                println!("  step {:4}: {:?}", k + 1, r.energies);
            }
        }
        let _ = model.energy("dissipated")?;
    }
    Ok(())
}
