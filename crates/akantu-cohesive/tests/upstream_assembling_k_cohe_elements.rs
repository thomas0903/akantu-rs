// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Port of
// test/test_model/test_solid_mechanics_model/test_cohesive/
// test_cohesive_extrinsic_implicit/test_assembling_K_cohe_elements.cc
// (+ K_matrix_verified.dat, quadrangle.msh, material.dat, copied verbatim to
// tests/data/upstream/).

//! TEST-222 / GOLD-006: `test_assembling_K_cohe_elements`.
//!
//! The upstream driver (its `add_akantu_test` is commented out in
//! `test_cohesive/CMakeLists.txt:29`) builds an **extrinsic** static cohesive
//! model on four `_quadrangle_4` (`quadrangle.msh`), limits the insertion
//! check to the facets with `y ∈ [-0.001, 0.001]`, blocks the bottom edge,
//! prescribes `u_y = 0.004` on the top-left node, solves one static step
//! (Newton–Raphson, at most 10 iterations, threshold 1e-13) and saves the
//! global stiffness `K` (`saveMatrix("K_matrix_test.dat")`, MatrixMarket,
//! upstream `digits10` precision, UD-061). It then compares `K` with the
//! committed `K_matrix_verified.dat` **its own way**: for every pair of
//! entries with the same `(row, col)`, the relative difference
//! `|K_test − K_ver| / |K_ver|` must be `≤ 1e-13`, or both values must be
//! `< 1e-13` in magnitude; entries present in only one file are not
//! compared. The verdict is the exit status (`EXIT_FAILURE` on a mismatch).
//!
//! The driver does not compile at the pinned commit (UD-697: removed
//! two-argument `SolidMechanicsModelCohesiveOptions` constructor); the oracle
//! runs it with the one-line API patch `oracle/probes/gold/patches/
//! test_assembling_K_cohe_elements.patch`, three times, on the x86_64 strict
//! install (`fixtures/gold/test_assembling_K_cohe_elements/`). This test
//! runs the same driver on the port and checks:
//!
//! 1. the saved `K_matrix_test.dat` text against the oracle's, **byte for
//!    byte**;
//! 2. the verdict of upstream's comparison with `K_matrix_verified.dat`
//!    (same comparison) against the oracle's exit status. The golden is
//!    stale (UD-697): both fail, with 99 pairs compared and 7 rejected (the
//!    rejected pairs are printed; with equal `K` bytes they are the oracle's).
//!
//! A second, port-only test documents where the golden comes from
//! (UD-697 (b)): forcing the extrinsic insertion check after the
//! static step (`checkCohesiveStress`) and solving again inserts the one
//! cohesive element of the 2015 golden. The current strict oracle aborts in
//! that second solve (`AssertException` "The value at position [18] is out
//! of range in array" in `SparseMatrixAIJ::applyBoundary`, exit 134, three
//! identical runs; the patched probe and its output are not committed),
//! which is UD-197 (registered, A); the
//! port, where UD-197 is defined behaviour, completes the step. Its `K` has
//! the golden's structure (20×20, node (−1, 0) doubled as DOFs 19–20), all
//! 118 golden entries are present, 108 agree within upstream's 1e-13, and
//! the 10 rejected are the state-dependent cohesive tangent entries on DOFs
//! 11, 12, 19 and 20. That test does not replace check 1, the decisive
//! differential check against the oracle.

mod common;

use std::path::PathBuf;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::Real;
use akantu_core::enums::AnalysisMethod;
use akantu_linalg::matrix_market::MatrixMarketOptions;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_validation::fixtures_root;

fn upstream_data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/upstream")
        .join(name)
}

fn oracle_output(name: &str) -> PathBuf {
    fixtures_root()
        .join("gold/test_assembling_K_cohe_elements")
        .join(name)
}

/// `getDOFManager().getMatrix("K").saveMatrix(…)` (upstream precision).
fn saved_k(model: &SolidMechanicsModelCohesive) -> String {
    model
        .smm
        .dof_manager()
        .matrix("K")
        .unwrap()
        .matrix_market_text_with(&MatrixMarketOptions::upstream_precision())
}

/// The driver up to `saveMatrix`: returns the model after the static step
/// and the `K_matrix_test.dat` text.
fn run_driver() -> (SolidMechanicsModelCohesive, String) {
    let spatial_dimension = 2;
    let increment: Real = 0.004;
    let mesh = common::read_mesh(&upstream_data("quadrangle.msh"), spatial_dimension);
    // `initialize("material.dat", argc, argv)`
    let parser = Parser::parse_file(upstream_data("material.dat")).unwrap();
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    // `initFull(SolidMechanicsModelCohesiveOptions(_static, true))`
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static), true)
        .unwrap();
    // `getElementInserter().setLimit(_y, -0.001, 0.001)`
    model.ext.inserter.set_limit(1, -0.001, 0.001).unwrap();
    model.update_automatic_insertion().unwrap();

    let position = mesh.read().unwrap().nodes().as_slice().to_vec();
    let nb_nodes = position.len() / spatial_dimension;
    {
        let mut boundary_g = model.smm.blocked_dofs_mut().unwrap();
        let boundary = boundary_g.as_mut_slice();
        for n in 0..nb_nodes {
            let (x, y) = (position[2 * n], position[2 * n + 1]);
            if y < -0.99 {
                boundary[2 * n + 1] = true;
                boundary[2 * n] = true;
            }
            if y > 0.99 && x < -0.99 {
                boundary[2 * n + 1] = true;
            }
        }
    }
    {
        let mut displacement_g = model.smm.displacement_mut().unwrap();
        let displacement = displacement_g.as_mut_slice();
        for n in 0..nb_nodes {
            let (x, y) = (position[2 * n], position[2 * n + 1]);
            if y > 0.99 && x < -0.99 {
                displacement[2 * n + 1] += increment;
            }
        }
    }
    {
        let solver = model.smm.solver.non_linear_solver_mut(None).unwrap();
        solver
            .set_param("max_iterations", ParamValue::Int(10))
            .unwrap();
        solver
            .set_param("threshold", ParamValue::Real(1e-13))
            .unwrap();
    }
    model.solve_step(None).unwrap();
    // `getDOFManager().getMatrix("K").saveMatrix("K_matrix_test.dat")`
    let k = saved_k(&model);
    (model, k)
}

/// One `(row, col, value)` line, read as upstream does
/// (`std::stringstream >> tmp(0) >> tmp(1) >> tmp(2)` into `Vector<Real>`).
type Entry = (Real, Real, Real);

/// Upstream's reader: the first line is skipped, the third number of the
/// second line is the number of entries, then that many lines are read.
fn read_entries(text: &str) -> Vec<Entry> {
    let mut lines = text.lines();
    lines.next();
    let header: Vec<Real> = lines
        .next()
        .unwrap()
        .split_whitespace()
        .map(|t| t.parse().unwrap())
        .collect();
    let nb_lines = header[2] as usize;
    (0..nb_lines)
        .map(|_| {
            let v: Vec<Real> = lines
                .next()
                .unwrap()
                .split_whitespace()
                .map(|t| t.parse().unwrap())
                .collect();
            (v[0], v[1], v[2])
        })
        .collect()
}

/// Upstream's comparison loop: the `(row, col)` pairs it compares and the
/// ones it rejects (in loop order); the program fails iff `rejected` is not
/// empty.
struct Verdict {
    compared: Vec<(Real, Real)>,
    rejected: Vec<(Real, Real, Real, Real)>,
}

fn upstream_comparison(k_verified: &[Entry], k_test: &[Entry]) -> Verdict {
    let tol: Real = 1.0e-13;
    let mut v = Verdict {
        compared: Vec::new(),
        rejected: Vec::new(),
    };
    for &(vi, vj, vk) in k_verified {
        for &(ti, tj, tk) in k_test {
            if ti == vi && tj == vj {
                v.compared.push((vi, vj));
                // `if (|K_ver| < tol) { if (|K_test| > tol) passed = false; }
                // else if (ratio > tol) passed = false;` (a NaN passes)
                let failed = if vk.abs() < tol {
                    tk.abs() > tol
                } else {
                    (tk - vk).abs() / vk.abs() > tol
                };
                if failed {
                    v.rejected.push((vi, vj, vk, tk));
                }
            }
        }
    }
    v
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn test_assembling_k_cohe_elements() {
    let (_, k_test_text) = run_driver();
    let k_verified = read_entries(&read(&upstream_data("K_matrix_verified.dat")));
    let k_test = read_entries(&k_test_text);
    let verdict = upstream_comparison(&k_verified, &k_test);
    let port_status = i32::from(!verdict.rejected.is_empty());
    println!(
        "K_matrix_test.dat: {} entries; compared with K_matrix_verified.dat \
         ({} entries): {} pairs compared, {} rejected; exit status {port_status}",
        k_test.len(),
        k_verified.len(),
        verdict.compared.len(),
        verdict.rejected.len()
    );
    for (i, j, ver, test) in &verdict.rejected {
        println!("  rejected ({i}, {j}): verified {ver:e}, test {test:e}");
    }
    let oracle_k = read(&oracle_output("K_matrix_test.dat"));
    assert_eq!(
        k_test_text, oracle_k,
        "K_matrix_test.dat differs from the oracle's"
    );
    let oracle_status: i32 = read(&oracle_output("exit_status")).trim().parse().unwrap();
    // UD-697: the stale golden
    assert_eq!((verdict.compared.len(), verdict.rejected.len()), (99, 7));
    assert_eq!(
        port_status, oracle_status,
        "verdict of upstream's comparison differs from the oracle's exit status"
    );
}

/// UD-697 (b), UD-197: the golden's scenario, one
/// inserted cohesive element, reproduced on the port by forcing the
/// extrinsic insertion check after the driver's static step and solving
/// again (the strict oracle aborts in that solve, UD-197). Port only: the
/// numbers below are the port's, checked against the stale golden with
/// upstream's own comparison.
#[test]
fn test_assembling_k_cohe_elements_after_forced_insertion() {
    let (mut model, k1) = run_driver();
    let header = |k: &str| k.lines().nth(1).unwrap().to_owned();
    assert_eq!(header(&k1), "18 18 107", "K of the driver's static step");
    // `model.checkCohesiveStress()`, then `model.solveStep()` again
    let inserted = model.check_cohesive_stress().unwrap();
    assert_eq!(
        inserted, 1,
        "one cohesive element inserted (as in the golden)"
    );
    model.solve_step(None).unwrap();
    let k2_text = saved_k(&model);
    // the golden is 20×20 (node (−1, 0) doubled as DOFs 19–20); the port
    // saves the general (unsymmetric) pattern
    assert_eq!(header(&k2_text), "20 20 216");
    let k_verified = read_entries(&read(&upstream_data("K_matrix_verified.dat")));
    assert_eq!(k_verified.len(), 118);
    let k2 = read_entries(&k2_text);
    let verdict = upstream_comparison(&k_verified, &k2);
    for (i, j, ver, test) in &verdict.rejected {
        println!("  rejected ({i}, {j}): verified {ver:e}, port {test:e}");
    }
    // every golden entry is present once in the port's K
    assert_eq!(verdict.compared.len(), 118, "golden entries found in K");
    assert_eq!(verdict.rejected.len(), 10, "108 of 118 within 1e-13");
    // the rejected entries are the cohesive tangent terms: both indices on
    // the DOFs of the crack faces' doubled node pair (11, 12 / 19, 20)
    let cohesive = |d: Real| [11.0, 12.0, 19.0, 20.0].contains(&d);
    for (i, j, _, _) in &verdict.rejected {
        assert!(
            cohesive(*i) && cohesive(*j),
            "rejected ({i}, {j}) outside the cohesive DOFs"
        );
    }
}
