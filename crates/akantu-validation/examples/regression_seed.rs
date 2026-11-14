// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: writes the `pending_oracle` seed of the committed regression
// fixture `regression/material/replay_elastic_2d`, which exercises the whole
// regression path (persist → `generate-fixtures.sh regression:<case_id>` →
// material probe → golden case).
//
//   cargo run -p akantu-validation --example regression_seed -- <fixtures_root>
//   oracle/scripts/generate-fixtures.sh regression:regression/material/replay_elastic_2d
//
// The test `regression_replay_round_trip` (tests/oracle_fixtures.rs)
// re-persists the same seed and checks the golden case against it.

use std::path::PathBuf;

use akantu_validation::manifest::Layout;
use akantu_validation::npy::{NpyArray, NpyData};
use akantu_validation::regression::{InputArray, RegressionCase, persist_regression_case};

/// Case id of the replayed fixture.
pub const CASE_ID: &str = "regression/material/replay_elastic_2d";

/// Material of the seed (plane strain, Akantu `.dat` syntax).
pub const MATERIAL: &str =
    "material elastic [\n  name = elastic\n  rho = 1.\n  E = 1e9\n  nu = 0.25\n]\n";

/// Prescribed displacement gradients `[step, row, col]` (asymmetric, so the
/// replay also exercises the tensor layout of the probe).
pub const GRAD_U: [f64; 12] = [
    1.0e-4, 2.0e-5, -3.0e-5, 5.0e-5, //
    2.0e-4, 4.0e-5, -6.0e-5, 1.0e-4, //
    -1.0e-4, 0.0, 7.0e-5, -2.0e-5,
];

/// The seed regression case.
#[must_use]
pub fn seed() -> RegressionCase {
    RegressionCase {
        case_id: CASE_ID.into(),
        description: "regression replay seed: elastic plane strain, asymmetric gradients".into(),
        origin: "examples/regression_seed.rs (replay-path exercise)".into(),
        probe: "material_probe".into(),
        parameters: serde_json::json!({
            "dim": 2,
            "material_dat": MATERIAL,
            "internals": [],
        }),
        input_files: Vec::new(),
        input_arrays: vec![InputArray {
            name: "grad_u".into(),
            array: NpyArray {
                shape: vec![3, 2, 2],
                data: NpyData::F64(GRAD_U.to_vec()),
            },
            axes: vec!["sample".into(), "row".into(), "col".into()],
            layout: Layout::Table,
        }],
    }
}

#[allow(dead_code)] // also compiled into tests/oracle_fixtures.rs, which only uses seed()
fn main() {
    let root = std::env::args()
        .nth(1)
        .map_or_else(akantu_validation::fixtures_root, PathBuf::from);
    match persist_regression_case(&root, &seed()) {
        Ok(dir) => println!("pending case written to {}", dir.display()),
        Err(e) => {
            eprintln!("regression_seed: {e}");
            std::process::exit(1);
        }
    }
}
