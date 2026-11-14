// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); differential test of
// the port of performance-testing-cohesive @ 483bace8 (cohesive_extrinsic.cc)
// against the strict C++ oracle with its MUMPS static solve.

//! The cohesive scalability driver with the MUMPS static solve (`_auto` on
//! the MPI world, feature `mumps`), run under `mpirun` (harness = false):
//!
//! * **1 rank** (rule C1P-1): against `fixtures/perf_cohesive_mumps_x86_64/coh_np1`
//!   (the strict oracle, MUMPS, OpenBLAS core Haswell, three byte-identical
//!   repetitions) **bitwise** — every energy of steps `0..=N` as the oracle's
//!   17-digit text, the insertion steps and counts, Δt — on x86_64 with the
//!   D-050 pin (`OPENBLAS_CORETYPE=Haswell`, one OpenBLAS thread); on any
//!   other platform or core the comparison is printed as a diagnostic only
//!   (as `mumps_mpi` does for its fixture sets);
//! * **more than one rank**: asserted are the solver kind
//!   (the static solve ran with MUMPS), MUMPS's process count (`nb_proc` =
//!   P, i.e. distributed, not centralised on rank 0), the number of energy
//!   rows, `ed` exactly equal to the serial reference's over steps
//!   `0..=min(N, 800)` and, at full length (`N = 1500`, release build), the
//!   counts (404 cohesive elements, 294 insertion steps, first at step 710)
//!   and the serial list of insertion steps (rule C1P-3). The serial
//!   reference is the 1-rank oracle (= the port's 1-rank run, C1P-1). `ep`,
//!   `ek`, `ew`, `et` against the serial reference are **printed, not
//!   asserted**: at P > 1 MUMPS's parallel factorisation is not
//!   reproducible from run to run (UD-348), and no bound has been derived
//!   (that needs the port's P-rank MUMPS input shown bitwise equal to
//!   upstream's and ≥ 20 repetitions per code); see
//!   `docs/NUMERICAL_VALIDATION.md` for the measured spread
//!   and why the former rule C1P-2 is withdrawn.
//!   Scope of `ed`: the serial reference has `ed` ≡ 0 up to step 838 (first
//!   opening at step 839), so over `0..=800` the check asserts that no
//!   cohesive element opens early; it is not a numerical comparison.
//!   Partitions: 2 and 4 ranks run on the C++ harness's Scotch partitions
//!   (`fixtures/perf_cohesive_x86_64/coh_np2_global/partition.csv`,
//!   `fixtures/perf_cohesive_mumps_x86_64/partition-np4.csv`, the partitions
//!   the C++ runs of the same problem use, so the printed values compare
//!   with theirs); other rank counts on RCB. The assertions are the same on
//!   every partition.
//!
//! `N` = 1500 in optimised builds, 200 otherwise (`PERF_STEPS=<n>` caps it).
//! Prints `failed=0` on success (the mumps tier's pass criterion).
//!
//! ```text
//! mpirun -n 1 target/.../perf_cohesive_mumps    # scripts/mumps/run-mumps-tier.sh
//! mpirun -n 2 target/.../perf_cohesive_mumps
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use akantu_bench::config::PartitionChoice;
use akantu_bench::output::g;
use akantu_bench::{Config, RunReport, run};
use akantu_comm::Communicator;

const COLUMNS: [&str; 5] = ["ed", "ep", "ek", "ew", "et"];
/// Window (last step) of the energy comparison with the serial reference.
const WINDOW: usize = 800;
/// Rule C1G-1 (PROPOSED; `docs/NUMERICAL_VALIDATION.md`): at P > 1 ranks, `ep` and `et` within 1e-12 relative of
/// the serial reference over the window — a **gross-error guard**, not a
/// solver-class bound (none is justified): ≈ 87× the 20-run maximum
/// of both codes at 4 ranks (C++ 1.14e-14 / 1.15e-14, Rust 1.15e-14 /
/// 1.17e-14), so outside the bimodal run-to-run class of the distributed
/// MUMPS factorisation (UD-348) and far below the effect of a wrong
/// distributed solve (e.g. one rank's partial entries lost).
const GROSS_ERROR: f64 = 1e-12;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn steps() -> usize {
    let n = if cfg!(debug_assertions) { 200 } else { 1500 };
    std::env::var("PERF_STEPS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .map_or(n, |c: usize| n.min(c))
}

/// Whether this run has the platform and OpenBLAS pin of the fixture.
fn pinned() -> bool {
    cfg!(target_arch = "x86_64")
        && std::env::var("OPENBLAS_CORETYPE").is_ok_and(|v| v.eq_ignore_ascii_case("haswell"))
        && std::env::var("OPENBLAS_NUM_THREADS").is_ok_and(|v| v.trim() == "1")
}

/// The oracle's `energies.csv` rows (text fields).
fn reference_energies() -> Vec<Vec<String>> {
    read(&fixtures().join("perf_cohesive_mumps_x86_64/coh_np1/energies.csv"))
        .lines()
        .skip(1)
        .map(|l| l.split(", ").map(str::to_owned).collect())
        .collect()
}

fn reference_insertions(steps: usize) -> Vec<(usize, usize)> {
    read(&fixtures().join("perf_cohesive_mumps_x86_64/coh_np1/insertions.csv"))
        .lines()
        .skip(1)
        .map(|l| {
            let f: Vec<usize> = l.split(", ").map(|x| x.parse().expect("count")).collect();
            (f[0], f[1])
        })
        .filter(|&(s, _)| s <= steps)
        .collect()
}

fn reference_time_step() -> f64 {
    let text = read(&fixtures().join("perf_cohesive_mumps_x86_64/coh_np1/summary.json"));
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("\"time_step\""))
        .expect("time_step");
    line.split(':')
        .nth(1)
        .expect("value")
        .trim()
        .trim_end_matches(',')
        .parse()
        .expect("Δt")
}

/// The C++ harness's exported Scotch partition for `np` ranks, if committed.
fn oracle_partition(np: usize) -> Option<PathBuf> {
    match np {
        2 => Some(fixtures().join("perf_cohesive_x86_64/coh_np2_global/partition.csv")),
        4 => Some(fixtures().join("perf_cohesive_mumps_x86_64/partition-np4.csv")),
        _ => None,
    }
}

/// The benchmark on `comm` (cohesive input with `blocked_node = global`).
fn run_case(comm: &Communicator, steps: usize) -> RunReport {
    let case = fixtures().join("perf_cohesive/coh_np2_global");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "perf_cohesive_mumps_np{}_r{}",
        comm.size(),
        comm.rank()
    ));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("output directory");
    let mut cfg = Config::from_file(&case.join("input.dat"), &case, &out).expect("input");
    cfg.max_steps = steps;
    cfg.verbose = false;
    cfg.export_partition = false;
    cfg.partitioner = match oracle_partition(comm.size()) {
        Some(path) => PartitionChoice::File(path),
        None => PartitionChoice::Rcb,
    };
    run(comm, &cfg).expect("run")
}

fn main() -> ExitCode {
    let world = match Communicator::mpi_world(akantu_comm::MpiOptions::default()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("perf_cohesive_mumps: no MPI world: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (np, root) = (world.size(), world.rank() == 0);
    let steps = steps();
    let report = run_case(&world, steps);
    let mut failed = 0_usize;
    if root {
        let mut fail = |msg: String| {
            eprintln!("FAIL: {msg}");
            failed += 1;
        };
        let s = &report.summary;
        if s.static_sparse_solver != "mumps" {
            fail(format!(
                "static solve with {}, expected mumps",
                s.static_sparse_solver
            ));
        }
        let nb_proc = s
            .static_solver_stats
            .iter()
            .find(|(k, _)| *k == "nb_proc")
            .map(|(_, v)| *v);
        if nb_proc != Some(np as f64) {
            fail(format!("MUMPS ran on {nb_proc:?} processes, expected {np}"));
        }
        let reference = reference_energies();
        let bitwise = np == 1 && pinned();
        let mut worst = [0.0_f64; 5];
        let mut first_text_difference = None;
        for (row, (step, values)) in reference.iter().zip(&report.energies) {
            assert_eq!(row[0], step.to_string(), "step numbering");
            for (k, v) in values.iter().enumerate() {
                let want = &row[k + 1];
                if first_text_difference.is_none() && &g(*v, 17) != want {
                    first_text_difference = Some((*step, COLUMNS[k]));
                }
                if *step <= WINDOW {
                    let w: f64 = want.parse().expect("oracle value");
                    let m = v.abs().max(w.abs());
                    worst[k] = worst[k].max(if m == 0.0 { 0.0 } else { (v - w).abs() / m });
                }
            }
        }
        if report.energies.len() != steps + 1 {
            fail(format!(
                "{} energy rows, expected {}",
                report.energies.len(),
                steps + 1
            ));
        }
        println!(
            "np={np} steps={steps} pinned={} max rel. difference vs the serial oracle, steps \
             0..=min({steps}, {WINDOW}): {}",
            pinned(),
            COLUMNS
                .iter()
                .zip(worst)
                .map(|(c, w)| format!("{c} {w:.3e}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        if bitwise {
            if let Some((step, col)) = first_text_difference {
                fail(format!("1 rank: not bitwise from step {step} ({col})"));
            }
            if report.summary.time_step.to_bits() != reference_time_step().to_bits() {
                fail(format!(
                    "Δt {} vs oracle {}",
                    report.summary.time_step,
                    reference_time_step()
                ));
            }
        } else if np == 1 {
            println!(
                "diagnostic (platform/OpenBLAS core other than the fixture's): first text \
                 difference {first_text_difference:?}"
            );
        } else {
            // `ed` is asserted, `ep`, `ek`, `ew`, `et` are printed
            // (diagnostic: run-to-run variation of the distributed MUMPS
            // factorisation, UD-348; no bound derived,
            // `docs/NUMERICAL_VALIDATION.md`), and `ep` / `et` checked against the
            // gross-error guard C1G-1.
            if worst[0] != 0.0 {
                fail(format!(
                    "{np} ranks: ed differs from the serial reference over steps 0..={} \
                     (max rel. {:.3e})",
                    steps.min(WINDOW),
                    worst[0]
                ));
            }
            // C1G-1: the gross-error guard of `ep` and `et` (NaN fails)
            for (k, name) in [(1, "ep"), (4, "et")] {
                if worst[k].is_nan() || worst[k] > GROSS_ERROR {
                    fail(format!(
                        "{np} ranks: {name} differs from the serial reference by {:.3e} \
                         (relative) over steps 0..={}: above the gross-error guard {GROSS_ERROR:e} \
                         (rule C1G-1)",
                        worst[k],
                        steps.min(WINDOW)
                    ));
                }
            }
            println!(
                "diagnostic ({} partition): ep {:.3e}, ek {:.3e}, ew {:.3e}, et {:.3e} (printed; \
                 ep and et within the gross-error guard C1G-1)",
                if oracle_partition(np).is_some() {
                    "the C++ harness's Scotch"
                } else {
                    "RCB"
                },
                worst[1],
                worst[2],
                worst[3],
                worst[4]
            );
        }
        // insertion steps: exact (1 rank always; every rank count at full length)
        if np == 1 || steps >= 1500 {
            if report.insertions != reference_insertions(steps) {
                let k = report
                    .insertions
                    .iter()
                    .zip(reference_insertions(steps))
                    .position(|(a, b)| *a != b);
                fail(format!(
                    "insertion steps differ from the serial reference (entry {k:?})"
                ));
            }
            if steps >= 1500 {
                let got = (
                    s.nb_cohesive_elements,
                    s.nb_insertion_steps,
                    s.first_insertion_step,
                );
                if got != (404, 294, 710) {
                    fail(format!("(cohesive, insertion steps, first) = {got:?}"));
                }
            }
        }
        println!("perf_cohesive_mumps np={np}: failed={failed}");
    }
    // the root's verdict decides the exit code of every rank
    let mut v = [u64::try_from(failed).unwrap_or(u64::MAX)];
    if world.broadcast(&mut v, 0).is_err() {
        return ExitCode::FAILURE;
    }
    drop(report);
    if v[0] == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
