// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/phase_field_parallel (port of
// examples/c++/phase_field_model/phase_field_parallel.cc, upstream ctest
// `phase_field_parallel-test`, 2 ranks).

mod examples_common;
#[path = "../examples/phase_field_parallel/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod phase_field_parallel;

use std::time::Duration;

use akantu::comm::Communicator;
use akantu::core::enums::SparseSolverType;
use examples_common::{files_below, oracle, scratch};
use phase_field_parallel as pfp;

/// One rank, reduced run (100 load steps), upstream's defects and the Eigen
/// solver pinned, against the oracle variant `phase_field_parallel_eigen_np1`:
/// the initial dump (the phase-field model's `phase.dump()` with the
/// partitions, upstream's always-distributed mesh) and the dump of step 100
/// byte-identical; the number of nodes printed.
#[test]
fn phase_field_parallel_one_rank_matches_the_oracle() {
    one_rank("phase_field_parallel", true);
}

/// [`phase_field_parallel_one_rank_matches_the_oracle`] in the default mode
/// (what `cargo run --example phase_field_parallel -- --solver eigen` runs
/// on one rank: corrected coupler, phase-field model and `phasefield`
/// tangent): the same oracle comparison holds
/// bitwise, the defects not being reached.
#[test]
fn phase_field_parallel_one_rank_default_mode_matches_the_oracle() {
    one_rank("phase_field_parallel_default", false);
}

fn one_rank(name: &str, compat: bool) {
    let out = scratch(name);
    let mut ex = pfp::example().for_test(&out);
    ex.steps = Some(100);
    ex.upstream_compat = compat;
    ex.sparse_solver = Some(SparseSolverType::Eigen);
    pfp::run_on(Communicator::serial(), &ex, &pfp::Partition::Default).unwrap();
    let o = oracle::oracle("phase_field_parallel_eigen_np1");
    assert_eq!(o.assert_files_match(&out, false, &|_| false), 2 * 2);
    assert!(ex.transcript().starts_with("Number of nodes: 151\n"));
    assert!(o.stdout().starts_with("Number of nodes: 151\n"));
}

/// Medium tier (optimised builds only): the whole one-rank run against the
/// oracle variant (10 dumps byte-identical, the standard output with the
/// wall-clock timings masked), and against the upstream binary on one rank
/// (`_auto` = MUMPS): its initial dump, before any solve, byte-identical.
#[cfg(not(debug_assertions))]
#[test]
fn phase_field_parallel_one_rank_full_run_matches_the_oracle() {
    let out = scratch("phase_field_parallel_full");
    let mut ex = pfp::example().for_test(&out);
    ex.upstream_compat = true;
    ex.sparse_solver = Some(SparseSolverType::Eigen);
    pfp::run_on(Communicator::serial(), &ex, &pfp::Partition::Default).unwrap();
    let o = oracle::oracle("phase_field_parallel_eigen_np1");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 10);
    assert_eq!(
        oracle::mask_numbers(&ex.transcript()),
        oracle::mask_numbers(&o.stdout())
    );
    let up = oracle::oracle("phase_field_parallel_np1");
    let n = up.assert_files_match(&out, false, &|f| {
        !(f.ends_with("_0000.vtu") || f.ends_with("_0000.pvtu"))
    });
    assert_eq!(n, 2);
}

/// The D-082 second check (distributed vs serial): the bound of
/// `par_phase_field::ENVELOPE` (`crates/akantu-couplers/tests/
/// par_phase_field/mod.rs`, decision D-082), relative to
/// the largest serial value.
const ENVELOPE: f64 = 1e-14;

/// Step-100 nodal state of one run, indexed by global node id:
/// displacement (2 per node) and the phase-field model's nodal damage.
#[derive(Clone, Debug)]
struct State {
    u: Vec<f64>,
    d: Vec<f64>,
}

/// `(global id, u, d)` of the nodes this rank owns (local or master: every
/// global node exactly once over the ranks; slaves and pure ghosts left
/// out).
type Owned = Vec<(usize, [f64; 2], f64)>;

fn owned_state(o: &pfp::Outcome) -> akantu::core::Result<Owned> {
    use akantu::core::{Error, Idx};
    use akantu::model::Model;
    let mesh = o.coupler.solid.mesh().read()?;
    let u = o.coupler.solid.displacement()?;
    let d = o.coupler.phase.damage()?;
    let (u, d) = (u.as_slice(), d.as_slice());
    let mut v = Vec::new();
    for n in 0..mesh.nb_nodes() {
        let i = Idx::try_from(n).map_err(|e| Error::invalid(format!("node {n}: {e}")))?;
        if mesh.is_local_or_master_node(i)? {
            let gid = usize::try_from(mesh.node_global_id(i))
                .map_err(|e| Error::invalid(format!("node {n}: global id: {e}")))?;
            v.push((gid, [u[2 * n], u[2 * n + 1]], d[n]));
        }
    }
    Ok(v)
}

/// The owned states of every rank, merged by global id; every one of the
/// `nb_nodes` global nodes appears exactly once.
fn merge(per_rank: Vec<Owned>, nb_nodes: usize) -> State {
    let mut u = vec![f64::NAN; 2 * nb_nodes];
    let mut d = vec![f64::NAN; nb_nodes];
    let mut seen = vec![false; nb_nodes];
    for (gid, ui, di) in per_rank.into_iter().flatten() {
        assert!(gid < nb_nodes, "global id {gid} of {nb_nodes}");
        assert!(!seen[gid], "global node {gid} owned twice");
        seen[gid] = true;
        u[2 * gid..2 * gid + 2].copy_from_slice(&ui);
        d[gid] = di;
    }
    assert!(seen.iter().all(|&s| s), "a global node is owned by no rank");
    State { u, d }
}

/// The port's one-rank run of the same reduced example (default mode,
/// Eigen, 100 steps), checked again here against
/// `phase_field_parallel_eigen_np1` (its two dumps byte-identical, as in
/// [`phase_field_parallel_one_rank_default_mode_matches_the_oracle`]):
/// the serial reference of the distributed runs, computed once per test
/// binary.
fn serial_state() -> &'static State {
    static SERIAL: std::sync::OnceLock<State> = std::sync::OnceLock::new();
    SERIAL.get_or_init(|| {
        let out = scratch("phase_field_parallel_serial_reference");
        let mut ex = pfp::example().for_test(&out);
        ex.steps = Some(100);
        ex.sparse_solver = Some(SparseSolverType::Eigen);
        let outcome = pfp::run_on(Communicator::serial(), &ex, &pfp::Partition::Default).unwrap();
        let o = oracle::oracle("phase_field_parallel_eigen_np1");
        assert_eq!(o.assert_files_match(&out, false, &|_| false), 2 * 2);
        let owned = owned_state(&outcome).unwrap();
        let nb_nodes = owned.len();
        assert_eq!(nb_nodes, 151);
        merge(vec![owned], nb_nodes)
    })
}

/// Largest absolute difference and largest absolute reference value.
fn max_diff(got: &[f64], want: &[f64]) -> (f64, f64) {
    assert_eq!(got.len(), want.len());
    got.iter()
        .zip(want)
        .fold((0.0_f64, 0.0_f64), |(d, m), (g, w)| {
            assert!(g.is_finite(), "non-finite value {g}");
            (d.max((g - w).abs()), m.max(w.abs()))
        })
}

/// The reduced example (100 load steps: dumps `_0000` at step 0 and `_0001`
/// at step 100, upstream's dumps 0 and 1) on `nb_ranks` local ranks with
/// the oracle's recorded partition (`partition_case`), the Eigen solver,
/// in the default and the compat mode:
/// - the two modes agree bitwise (the switches are inert in a solve, as on
///   one rank);
/// - against the upstream binary's own run at that rank count (`case`),
///   on what is deterministic there: the written file names are exactly
///   upstream's for dumps 0 and 1; `_0000.proc<r>.vtu`, `_0000.pvtu` and
///   `_0001.pvtu` byte-identical; positions, connectivity, offsets, types
///   and partitions of `_0001.proc<r>.vtu` identical (all flagged
///   deterministic in its `vtu_arrays.txt`);
/// - the step-100 nodal displacement and damage, merged by global node id,
///   within [`ENVELOPE`] of the port's one-rank run ([`serial_state`]; the
///   D-082 second check), measured values printed.
///
/// Upstream's own step-100 values at 2 / 4 ranks are run-dependent
/// ([`assert_upstream_run_dependent`]), so they are not compared.
fn distributed(nb_ranks: usize, case: &str, partition_case: &str) {
    let o = oracle::oracle(partition_case);
    let parts = pfp::read_partition(&o.kept("partition.txt")).unwrap();
    let partition = pfp::Partition::Prescribed(parts);
    let up = oracle::oracle(case);
    let serial = serial_state();
    let nb_nodes = serial.d.len();
    let base = "paraview/phase_notch_parallel";
    let mut want_files: Vec<String> = (0..2)
        .flat_map(|dump| {
            (0..nb_ranks)
                .map(move |r| format!("{base}-VTU/phase_notch_parallel_{dump:04}.proc{r}.vtu"))
                .chain(std::iter::once(format!("{base}_{dump:04}.pvtu")))
        })
        .collect();
    want_files.sort();
    for f in &want_files {
        assert!(up.digests.contains_key(f), "{case}: upstream wrote no {f}");
    }
    let mut states = Vec::new();
    for compat in [false, true] {
        let mode = if compat { "compat" } else { "default" };
        let out = scratch(&format!("phase_field_parallel_np{nb_ranks}_{mode}"));
        let started = std::time::Instant::now();
        let results = akantu::comm::run_local(nb_ranks, Duration::from_secs(3600), |c| {
            let mut ex = pfp::example().for_test(&out);
            ex.steps = Some(100);
            ex.upstream_compat = compat;
            ex.sparse_solver = Some(SparseSolverType::Eigen);
            let outcome = pfp::run_on(c, &ex, &partition).map_err(|e| e.to_string())?;
            owned_state(&outcome).map_err(|e| e.to_string())
        });
        let elapsed = started.elapsed();
        let per_rank: Vec<Owned> = results
            .into_iter()
            .enumerate()
            .map(|(r, x)| x.unwrap_or_else(|e| panic!("{nb_ranks} ranks ({mode}), rank {r}: {e}")))
            .collect();
        assert_eq!(files_below(&out), want_files, "{mode}: written files");
        let n = up.assert_files_match(&out, false, &|f| f.contains("_0001.proc"));
        assert_eq!(n, nb_ranks + 2, "{mode}: files compared byte for byte");
        for r in 0..nb_ranks {
            let rel = format!("{base}-VTU/phase_notch_parallel_0001.proc{r}.vtu");
            let k = up.assert_vtu_arrays_match(&out, &rel, &|a| {
                matches!(a, "displacement" | "damage" | "grad_u" | "stress")
            });
            assert_eq!(k, 5, "{mode}: {rel}: arrays compared");
        }
        println!(
            "INFO phase_field_parallel {nb_ranks} ranks ({mode}): 100 steps in {:.1} s",
            elapsed.as_secs_f64()
        );
        states.push(merge(per_rank, nb_nodes));
    }
    let (default, compat) = (&states[0], &states[1]);
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    assert_eq!(
        bits(&default.u),
        bits(&compat.u),
        "displacement: default vs compat"
    );
    assert_eq!(
        bits(&default.d),
        bits(&compat.d),
        "damage: default vs compat"
    );
    let (du, mu) = max_diff(&default.u, &serial.u);
    let (dd, md) = max_diff(&default.d, &serial.d);
    let exact = |a: &[f64], b: &[f64]| {
        a.iter()
            .zip(b)
            .filter(|(x, y)| x.to_bits() == y.to_bits())
            .count()
    };
    println!(
        "INFO phase_field_parallel {nb_ranks} ranks vs one rank, step 100: |du|max = {du:.3e} \
         (|u|max = {mu:.3e}, {:.2e} relative, {}/{} bitwise), |dd|max = {dd:.3e} (|d|max = \
         {md:.3e}, {:.2e} relative, {}/{} bitwise); bound {ENVELOPE:e} relative (D-082)",
        du / mu,
        exact(&default.u, &serial.u),
        serial.u.len(),
        dd / md,
        exact(&default.d, &serial.d),
        serial.d.len(),
    );
    assert!(
        md > 0.0 && mu > 0.0,
        "no damage or no displacement developed"
    );
    assert!(
        du <= ENVELOPE * mu,
        "displacement at {nb_ranks} ranks: |du|max {du:e} > {ENVELOPE:e} · {mu:e}"
    );
    assert!(
        dd <= ENVELOPE * md,
        "damage at {nb_ranks} ranks: |dd|max {dd:e} > {ENVELOPE:e} · {md:e}"
    );
}

/// The oracle's two- and four-rank runs (`phase_field_parallel`,
/// `phase_field_parallel_np4`, upstream binary with MUMPS) are not
/// reproducible run to run: 18 of their 30 (36 of 50) files differ
/// between three identical runs (the partitioned dumps after the first
/// solves; cause not established: the distributed MUMPS solves and the
/// arrival order of upstream's ghost accumulations, `waitAny`, D-039, were
/// not separated; the UD-004 ghost damage gradients also enter). Three runs
/// are no D-082 variation study (≥ 12 runs, cause a solver the port does
/// not reproduce), so the port's 2 / 4-rank values are compared with its
/// own serial run, not with these; the D-082 coverage of
/// the same coupler and material file is MPI-044's `phase_field_square_n8`.
fn assert_upstream_run_dependent() {
    for (case, nondet, total) in [
        ("phase_field_parallel", 18, 30),
        ("phase_field_parallel_np4", 36, 50),
    ] {
        let o = oracle::oracle(case);
        assert_eq!(
            (o.nondeterministic.len(), o.digests.len()),
            (nondet, total),
            "{case}"
        );
    }
}

/// Two ranks (the upstream ctest's count), debug tier: see [`distributed`];
/// the oracle's run dependence ([`assert_upstream_run_dependent`]) is the
/// reason no value is compared with upstream's 2 / 4-rank runs.
#[test]
fn phase_field_parallel_two_ranks() {
    assert_upstream_run_dependent();
    distributed(2, "phase_field_parallel", "phase_field_parallel_partition");
}

/// Four ranks: see [`distributed`].
#[test]
fn phase_field_parallel_four_ranks() {
    distributed(
        4,
        "phase_field_parallel_np4",
        "phase_field_parallel_partition_np4",
    );
}
