// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: `SolidMechanicsModel::energies` (one reduction of several
// energies, D-126) returns, for every id, bitwise the value of
// `SolidMechanicsModel::energy(id)` under the default rank-ordered reduction
// policy, on 1, 2 and 4 ranks of the local world, explicit and static.

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_smm;

use std::time::Duration;

use akantu_comm::run_local;
use akantu_core::enums::AnalysisMethod;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use par_smm::{Part, ok};

const IDS: [&str; 5] = [
    "kinetic",
    "potential",
    "external work",
    "dissipated",
    "external work new",
];

fn check(np: usize, method: AnalysisMethod) {
    let out = run_local(np, Duration::from_secs(120), |comm| {
        let m = par_smm::distributed_cube(&comm, 3, &Part::Rcb)?;
        let shared = SharedMesh::new(m);
        let parser = ok(Parser::parse_str(par_smm::DAT, "material.dat"), "parser")?;
        let mut model = ok(
            SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
            "model",
        )?;
        ok(model.init_full(&ModelOptions::new(method)), "init")?;
        let (coords, nb) = {
            let mm = ok(shared.read(), "read")?;
            (mm.nodes().as_slice().to_vec(), mm.nb_nodes())
        };
        {
            let mut b_g = ok(model.blocked_dofs_mut(), "blocked")?;
            let b = b_g.as_mut_slice();
            for n in 0..nb {
                if coords[3 * n + 2] == 0.0 {
                    b[3 * n..3 * n + 3].fill(true);
                }
            }
        }
        {
            let mut f_g = ok(model.external_force_mut(), "f")?;
            let f = f_g.as_mut_slice();
            for n in 0..nb {
                if coords[3 * n + 2] == 1.0 {
                    f[3 * n] = 1e-3 * (1.0 + coords[3 * n + 1]);
                    f[3 * n + 2] = -2e-3 * (1.0 + 0.37 * coords[3 * n]);
                }
            }
        }
        if method != AnalysisMethod::Static {
            let dt = ok(model.stable_time_step(), "dt")? * 0.3;
            ok(model.set_time_step(dt, None), "set dt")?;
            let mut v_g = ok(model.velocity_mut(), "v")?;
            for (i, v) in v_g.as_mut_slice().iter_mut().enumerate() {
                *v = 1e-4 * ((i * 7919 % 101) as f64 / 101.0 - 0.5);
            }
        }
        // static: no mass, no kinetic energy
        let ids: Vec<&str> = IDS
            .iter()
            .copied()
            .filter(|id| method != AnalysisMethod::Static || *id != "kinetic")
            .collect();
        let sub: Vec<&str> = ["external work", "kinetic", "external work", "potential"]
            .into_iter()
            .filter(|id| method != AnalysisMethod::Static || *id != "kinetic")
            .collect();
        let mut seen = Vec::new();
        for step in 0..4 {
            ok(model.solve_step(None), "solve")?;
            let one: Vec<u64> = ids
                .iter()
                .map(|id| model.energy(id).map(f64::to_bits))
                .collect::<akantu_core::Result<_>>()
                .map_err(|e| e.to_string())?;
            let all: Vec<u64> = ok(model.energies(&ids), "energies")?
                .into_iter()
                .map(f64::to_bits)
                .collect();
            // also a permuted, repeated subset
            let one_sub: Vec<u64> = sub
                .iter()
                .map(|id| model.energy(id).map(f64::to_bits))
                .collect::<akantu_core::Result<_>>()
                .map_err(|e| e.to_string())?;
            let all_sub: Vec<u64> = ok(model.energies(&sub), "energies")?
                .into_iter()
                .map(f64::to_bits)
                .collect();
            if one != all || one_sub != all_sub {
                return Err(format!(
                    "step {step}: energy(id) {one:x?} / {one_sub:x?}, energies {all:x?} / {all_sub:x?}"
                ));
            }
            seen.push(one);
        }
        // the energies are not trivially zero (the check means something)
        let nonzero = seen
            .iter()
            .flatten()
            .filter(|b| f64::from_bits(**b) != 0.0)
            .count();
        Ok::<_, String>(nonzero)
    });
    for (rank, r) in out.into_iter().enumerate() {
        let nonzero = r.unwrap_or_else(|e| panic!("np {np}, {method:?}, rank {rank}: {e}"));
        assert!(
            nonzero >= 4,
            "np {np}, {method:?}: only {nonzero} nonzero energies"
        );
    }
}

#[test]
fn energies_equal_energy_bitwise_explicit() {
    for np in [1, 2, 4] {
        check(np, AnalysisMethod::ExplicitLumpedMass);
    }
}

#[test]
fn energies_equal_energy_bitwise_static() {
    for np in [1, 2, 4] {
        check(np, AnalysisMethod::Static);
    }
}
