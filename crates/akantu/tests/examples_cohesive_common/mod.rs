// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test helpers of the
// integration tests of the cohesive examples
// (tests/examples_cohesive_*.rs, tests/examples_scalability_test.rs):
// comparisons with the oracle runs of oracle/probes/examples (standard
// output, 17-digit energy records of the `*_full` variants, consistency of
// the Eigen-pinned variants with the upstream binaries).

#![allow(dead_code)] // each test binary uses a subset of the helpers

use std::path::Path;

use crate::examples_common::oracle::{OracleCase, assert_csv_values_bitwise, oracle};

/// The standard output of the oracle case `name` (run 1), line by line.
pub fn oracle_stdout(name: &str) -> Vec<String> {
    let o = oracle(name);
    std::fs::read_to_string(o.dir.join("stdout.txt"))
        .unwrap_or_else(|e| panic!("oracle case {name}: stdout.txt: {e}"))
        .lines()
        .map(str::to_owned)
        .collect()
}

/// `got` (the example's transcript) equals the oracle's standard output,
/// except the lines for which `skip` returns true (on both sides: timings).
pub fn assert_stdout_matches(name: &str, got: &[String], skip: &dyn Fn(&str) -> bool) {
    let want: Vec<String> = oracle_stdout(name)
        .into_iter()
        .filter(|l| !skip(l))
        .collect();
    let got: Vec<&String> = got.iter().filter(|l| !skip(l)).collect();
    assert_eq!(
        got.len(),
        want.len(),
        "{name}: number of stdout lines\n--- got\n{got:?}\n--- want\n{want:?}"
    );
    for (k, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(*g, w, "{name}: stdout line {k}");
    }
}

/// One line of an oracle `energy_full.csv`
/// (`step,nb_cohesive,dissipated,reversible,potential,kinetic`).
#[derive(Clone, Debug)]
pub struct OracleEnergy {
    /// Step.
    pub step: usize,
    /// Number of cohesive elements after the step.
    pub nb_cohesive: usize,
    /// The four energies, as printed (17 significant digits).
    pub energies: [String; 4],
}

/// The records of the oracle variant `name` (`energy_full.csv`).
pub fn oracle_energies(name: &str) -> Vec<OracleEnergy> {
    let o = oracle(name);
    let lines = o.kept_lines("energy_full.csv");
    assert_eq!(
        lines[0], "step,nb_cohesive,dissipated,reversible,potential,kinetic",
        "{name}: energy_full.csv header"
    );
    lines[1..]
        .iter()
        .map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            assert_eq!(c.len(), 6, "{name}: energy_full.csv line {l}");
            OracleEnergy {
                step: c[0].parse().unwrap(),
                nb_cohesive: c[1].parse().unwrap(),
                energies: [c[2], c[3], c[4], c[5]].map(str::to_owned),
            }
        })
        .collect()
}

/// `got` (`(step, nb_cohesive, [dissipated, reversible, potential,
/// kinetic])` after every step) equals the first `got.len()` records of the
/// oracle variant `name`: step numbers and element counts exactly, the
/// energies bitwise.
pub fn assert_energies_bitwise(name: &str, got: &[(usize, usize, [f64; 4])]) {
    let want = oracle_energies(name);
    assert!(
        got.len() <= want.len(),
        "{name}: {} records, the oracle has {}",
        got.len(),
        want.len()
    );
    for ((step, nb, e), w) in got.iter().zip(&want) {
        assert_eq!(*step, w.step, "{name}: step");
        assert_eq!(
            *nb, w.nb_cohesive,
            "{name}: cohesive elements after step {step}"
        );
        let w: Vec<&str> = w.energies.iter().map(String::as_str).collect();
        assert_csv_values_bitwise(&format!("{name}: energies after step {step}"), e, &w);
    }
}

/// The output files of the Eigen-pinned oracle variant `variant` below
/// `prefix` are those of the upstream binary's case `upstream`, with the
/// same SHA-256 (D-050: the pin changes nothing in an explicit run, so the
/// unpinned upstream binary's outputs are golden too). Returns the number
/// of files compared.
pub fn assert_variant_matches_upstream(upstream: &str, variant: &str, prefix: &str) -> usize {
    let (u, v): (OracleCase, OracleCase) = (oracle(upstream), oracle(variant));
    let files = |o: &OracleCase| -> Vec<(String, String)> {
        o.digests
            .iter()
            .filter(|(f, _)| f.starts_with(prefix))
            .map(|(f, h)| (f.clone(), h.clone()))
            .collect()
    };
    let (fu, fv) = (files(&u), files(&v));
    assert!(!fu.is_empty(), "{upstream}: no output below {prefix}");
    assert_eq!(fu, fv, "{upstream} vs {variant}: outputs below {prefix}");
    assert!(
        u.nondeterministic.is_empty(),
        "{upstream}: nondeterministic outputs"
    );
    assert!(
        v.nondeterministic.is_empty(),
        "{variant}: nondeterministic outputs"
    );
    fu.len()
}

/// The oracle ran its case `name` three times with exit code 0 (D-035).
pub fn assert_oracle_status(name: &str) {
    let o = oracle(name);
    let status = std::fs::read_to_string(o.dir.join("status.txt")).unwrap();
    assert!(
        status.contains("runs: 3, exit codes: 0 0 0"),
        "{name}: {status}"
    );
    assert!(
        status.contains("nondeterministic files: 0"),
        "{name}: {status}"
    );
}

/// The frozen input of an oracle variant (`oracle/probes/examples/<file>`).
pub fn probe_input(file: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/probes/examples")
        .join(file)
}
