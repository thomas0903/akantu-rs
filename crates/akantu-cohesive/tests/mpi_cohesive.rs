// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). real-MPI
// scenarios (ported intent: test/test_model/test_solid_mechanics_model/
// test_cohesive/test_parallel_cohesive, unregistered upstream): the
// SolidMechanicsModelCohesive on a distributed mesh under mpirun.

//! `mpirun -np N mpi_cohesive` (harness = false;
//! scripts/mpi/run-par-tests.sh): `PASS`/`FAIL` lines for the scenarios
//! that ran, `NOT RUN <name>: <why>` for those that do not run at this
//! rank count and a `SUMMARY … not_run=K` on rank 0
//! (`scenarios=` counts the scenarios that ran); exit 1 on failure.
//! Independent checks at every rank count (RCB
//! partitions; 8 ranks in the expensive tier); oracle replays at every rank
//! count the probe recorded (1, 2 and 4; 8 for the 2-D intrinsic and
//! extrinsic scenarios) with the rank-ordered and the
//! NativeMpi reduction policies (the
//! latter compares the globally reduced energies with upstream's
//! `MPI_Allreduce` bitwise), including the scenarios
//! (facets-only insertion, clustering, fragments; under NativeMpi every
//! fragment sum is compared with upstream's bitwise). At every rank count:
//! UD-1141's refusal of a distributed 1-D facet mesh.

mod common;
mod par_coh;

use std::time::Duration;

use akantu_cohesive::par_scenarios;
use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, ReductionPolicy, SendMode};

fn world_with(reduction: ReductionPolicy) -> Communicator {
    let s = std::env::var("AKANTU_MPI_TEST_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);
    Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(Duration::from_secs(s)),
        send_mode: SendMode::Eager,
        reduction,
    })
    .expect("MPI world communicator")
}

type S = Box<dyn Fn(Communicator) -> par_coh::Outcome>;

/// Bound of a run with an injected failure: well under the world's
/// per-operation timeout (no rank waited for it).
const FAILURE_BOUND: Duration = Duration::from_secs(30);

/// `par_coh::checks::FAILURES` entry `name` under both modes, failing on
/// the first then on the last rank.
fn collective_failure(c: &Communicator, name: &str) -> par_coh::Outcome {
    use par_coh::checks;
    let &(_, pattern, nth, fatal) = checks::FAILURES
        .iter()
        .find(|f| f.0 == name)
        .ok_or("unknown scenario")?;
    let mode0 = c.collective_checks();
    let rank = usize::try_from(c.rank()).map_err(|e| e.to_string())?;
    let r = (|| {
        for mode in checks::MODES {
            for failing in [0, c.size() - 1] {
                let what = format!("{pattern} #{nth}, {mode:?}, rank {failing} failing");
                let s = checks::failure_run(c, mode, pattern, nth, failing);
                checks::check_failure(&s, rank, failing, fatal, FAILURE_BOUND, &what)?;
                // the same committed revisions on every rank
                let mut lo = [s.revision];
                let mut hi = [s.revision];
                par_coh::ok(c.all_reduce(&mut lo, ReduceOp::Min), "revision min")?;
                par_coh::ok(c.all_reduce(&mut hi, ReduceOp::Max), "revision max")?;
                if lo != hi {
                    return Err(format!(
                        "{what}: committed mesh revisions differ between ranks ({} to {})",
                        lo[0], hi[0]
                    ));
                }
            }
        }
        Ok(())
    })();
    c.set_collective_checks(mode0);
    r
}

/// `SolverVector::norm_in` on a communicator split to one rank, under both
/// modes, after a deferred outcome (D-126: nothing is deferred on a one-rank
/// communicator; the norm's reduction is issued there too).
fn norm_on_a_one_rank_split(c: &Communicator) -> par_coh::Outcome {
    use akantu_comm::phase::{Phases, Severity};
    let rank = u32::try_from(c.rank()).map_err(|e| e.to_string())?;
    let solo = par_coh::ok(c.split(Some(rank), 0), "split")?.ok_or("split: no communicator")?;
    if solo.size() != 1 {
        return Err(format!("split: {} ranks", solo.size()));
    }
    let mode0 = c.collective_checks();
    let r = (|| {
        for mode in par_coh::checks::MODES {
            solo.set_collective_checks(mode);
            let mut v = akantu_linalg::SolverVector::new("v", false);
            v.resize(4);
            v.set(2.0);
            let mut ph = Phases::settling(&solo, "norm");
            par_coh::ok(ph.defer("local", Ok(()), Severity::Recoverable), "defer")?;
            let n = v.norm_in(&mut ph, "norm", &[true, true, true, false]);
            let n = par_coh::ok(ph.settle(n), "norm")?;
            if n.to_bits() != 12.0_f64.sqrt().to_bits() {
                return Err(format!("{mode:?}: norm {n}"));
            }
        }
        Ok(())
    })();
    c.set_collective_checks(mode0);
    r
}

/// The persistent state after a failure in each phase of the explicit
/// solve step, `offset` steps after the first insertion, is the
/// every-phase mode's (`par_coh::checks::check_states`).
fn collective_state(c: &Communicator, offset: usize) -> par_coh::Outcome {
    use par_coh::checks;
    let mode0 = c.collective_checks();
    let rank = usize::try_from(c.rank()).map_err(|e| e.to_string())?;
    let r = checks::check_states(
        rank,
        &[0, c.size() - 1],
        &mut |mode, inject, failing| checks::state_run(c, offset, mode, inject, failing),
        &format!("step +{offset}"),
    );
    c.set_collective_checks(mode0);
    let n = r?;
    if rank == 0 {
        println!("INFO collective_checks_state_step_plus_{offset}: {n} injected failures compared");
    }
    Ok(())
}

fn main() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(900));
        eprintln!("WATCHDOG: no completion after 900 s, aborting");
        std::process::abort();
    });
    let control = world_with(ReductionPolicy::RankOrdered);
    let native = world_with(ReductionPolicy::NativeMpi);
    let size = control.size();
    let mut scenarios: Vec<(String, S, bool)> = Vec::new();
    // A scenario that does not run at this rank count is
    // not listed (no PASS, not counted) and says why
    let mut not_run: Vec<(String, String)> = Vec::new();
    let not_recorded = |name: &str| {
        format!(
            "no oracle fixture at {size} ranks (fixtures/parallel_cohesive/{name}: np{:?})",
            par_coh::oracle_rank_counts(name)
        )
    };
    // Every replayed fixture is recorded at some count
    // (an unreadable fixtures root would otherwise drop every replay)
    let missing: Vec<&str> = par_scenarios::specs()
        .iter()
        .map(|s| s.name)
        .chain([
            "facets_only_tri",
            "facets_only_tet",
            "facets_only_pg_tri",
            "facets_only_bnd_tri",
            "clusters_tri",
            "clusters_chk_tri",
            "fragments_tri",
        ])
        .filter(|n| par_coh::oracle_rank_counts(n).is_empty())
        .collect();
    if !missing.is_empty() {
        println!(
            "FAIL start-up (rank {}): no recorded rank count under fixtures/parallel_cohesive \
             for {missing:?}",
            control.rank()
        );
        drop(native);
        drop(control);
        std::process::exit(1);
    }
    // UD-1141: a distributed 1-D facet mesh is refused
    // on every rank (built on one rank), the mesh and the communicator
    // usable afterwards
    scenarios.push((
        "distributed_1d_facet_mesh_refused".into(),
        Box::new(|c| common::distributed_1d_refusal(&c).map(|_| ())),
        false,
    ));
    for spec in par_scenarios::specs() {
        let s = spec.clone();
        scenarios.push((
            format!("invariants_rcb_{}", spec.name),
            Box::new(move |c| par_coh::invariants(&s, &c, par_coh::PartKind::Rcb)),
            false,
        ));
    }
    // the oracle replays run at every rank count the probe recorded
    // (fixtures/parallel_cohesive/<scenario>/np<P>: 1, 2 and 4 ranks, and 8
    // ranks for the 2-D scenarios)
    let recorded = |name: &str| par_coh::oracle_rank_counts(name).contains(&size);
    for spec in par_scenarios::specs() {
        let name = spec.name;
        if recorded(name) {
            scenarios.push((
                format!("{name}_matches_oracle"),
                Box::new(move |c| par_coh::matches_oracle(name, &c)),
                false,
            ));
            scenarios.push((
                format!("{name}_matches_oracle_native_mpi"),
                Box::new(move |c| par_coh::matches_oracle(name, &c)),
                true,
            ));
        } else {
            for sfx in ["", "_native_mpi"] {
                not_run.push((format!("{name}_matches_oracle{sfx}"), not_recorded(name)));
            }
        }
        // the probe's partition rules: 2 and 4 ranks (library), 8 ranks in
        // 2-D (par_coh::part_2d_8)
        if matches!(size, 2 | 4) || (size == 8 && spec.dim == 2) {
            let s = spec.clone();
            scenarios.push((
                format!("invariants_rule_{name}"),
                Box::new(move |c| par_coh::invariants(&s, &c, par_coh::PartKind::Rule)),
                false,
            ));
        } else {
            not_run.push((
                format!("invariants_rule_{name}"),
                format!(
                    "the probe's partition rule exists for 2 and 4 ranks (8 in 2-D), not for \
                     {size} ranks in {}-D",
                    spec.dim
                ),
            ));
        }
    }
    // Facets-only insertion, clustering and fragments
    for with_native in [false, true] {
        let sfx = if with_native { "_native_mpi" } else { "" };
        for name in [
            "facets_only_tri",
            "facets_only_tet",
            // adversarial cases (bnd)
            "facets_only_pg_tri",
            "facets_only_bnd_tri",
        ] {
            if recorded(name) {
                scenarios.push((
                    format!("{name}_matches_oracle{sfx}"),
                    Box::new(move |c| par_coh::coh2::facets_only_matches_oracle(name, &c)),
                    with_native,
                ));
            } else {
                not_run.push((format!("{name}_matches_oracle{sfx}"), not_recorded(name)));
            }
        }
        if recorded("clusters_tri") {
            scenarios.push((
                format!("clusters_tri_matches_oracle{sfx}"),
                Box::new(|c| par_coh::coh2::clusters_matches_oracle(&c)),
                with_native,
            ));
        } else {
            not_run.push((
                format!("clusters_tri_matches_oracle{sfx}"),
                not_recorded("clusters_tri"),
            ));
        }
        if recorded("clusters_chk_tri") {
            scenarios.push((
                format!("clusters_chk_tri_matches_oracle{sfx}"),
                Box::new(|c| par_coh::coh2::clusters_chk_matches_oracle(&c)),
                with_native,
            ));
        } else {
            not_run.push((
                format!("clusters_chk_tri_matches_oracle{sfx}"),
                not_recorded("clusters_chk_tri"),
            ));
        }
        if recorded("fragments_tri") {
            scenarios.push((
                format!("fragments_tri_matches_oracle{sfx}"),
                Box::new(|c| par_coh::coh2::fragments_matches_oracle(&c).map(|_| ())),
                with_native,
            ));
        } else {
            not_run.push((
                format!("fragments_tri_matches_oracle{sfx}"),
                not_recorded("fragments_tri"),
            ));
        }
    }
    scenarios.push((
        "solver_vector_norm_on_a_one_rank_split".into(),
        Box::new(|c| norm_on_a_one_rank_split(&c)),
        false,
    ));
    // The collective error protocol under both collective-check modes
    // (D-018, D-126): a failure injected on one rank (the first, then the
    // last) in chosen phases of the steps, and the persistent state after a
    // failure in each phase of the explicit solve step (the scenario's
    // partition rule exists for 2 and 4 ranks)
    if matches!(size, 2 | 4) {
        for &(name, ..) in &par_coh::checks::FAILURES {
            scenarios.push((
                format!("collective_checks_{name}"),
                Box::new(move |c| collective_failure(&c, name)),
                false,
            ));
        }
        for offset in [0, 2] {
            scenarios.push((
                format!("collective_checks_state_step_plus_{offset}"),
                Box::new(move |c| collective_state(&c, offset)),
                false,
            ));
        }
    } else {
        for &(name, ..) in &par_coh::checks::FAILURES {
            not_run.push((
                format!("collective_checks_{name}"),
                format!("the scenario's partition rule exists for 2 and 4 ranks, not {size}"),
            ));
        }
        for offset in [0, 2] {
            not_run.push((
                format!("collective_checks_state_step_plus_{offset}"),
                format!("the scenario's partition rule exists for 2 and 4 ranks, not {size}"),
            ));
        }
    }
    if control.rank() == 0 {
        for (name, why) in &not_run {
            println!("NOT RUN {name}: {why}");
        }
    }
    let mut failed = 0u64;
    let count = scenarios.len();
    for (name, f, nat) in &scenarios {
        let comm = if *nat {
            native.duplicate()
        } else {
            control.duplicate()
        }
        .expect("duplicate");
        let t0 = std::time::Instant::now();
        let r = f(comm);
        let mut bad = [u64::from(r.is_err())];
        control
            .all_reduce(&mut bad, ReduceOp::Sum)
            .expect("all_reduce");
        if let Err(e) = &r {
            println!("FAIL {name} (rank {}): {e}", control.rank());
        }
        if bad[0] > 0 {
            failed += 1;
        } else if control.rank() == 0 {
            println!("PASS {name} ({:.2} s)", t0.elapsed().as_secs_f64());
        }
    }
    if control.rank() == 0 {
        println!(
            "SUMMARY ranks={size} scenarios={count} failed={failed} not_run={}",
            not_run.len()
        );
    }
    drop(native);
    drop(control);
    std::process::exit(i32::from(failed > 0));
}
