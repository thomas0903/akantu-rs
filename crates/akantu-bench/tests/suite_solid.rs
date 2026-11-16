// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the
// benchmark suite's solid cases; no upstream counterpart.

//! Every `suite_solid` case at size `s`, twice: it completes, records its
//! phases, and its digest repeats. The step split used by the cases (the
//! model's intercepting callback) computes the same bits as `solve_step`.

use akantu_bench::suite::{Options, Outcome, Size, run_case};
use akantu_bench::suite_solid::common::{self, StepSplit};
use akantu_bench::suite_solid::mesh::{BoxSpec, box_mesh};
use akantu_comm::Communicator;
use akantu_core::ElementType;
use akantu_core::enums::AnalysisMethod;

fn opts() -> Options {
    Options {
        cases: vec!["all".into()],
        size: Size::S,
        reps: 1,
        out: None,
        loop_seconds: None,
        threads: 1,
    }
}

#[test]
fn every_case_runs_at_size_s_and_its_digest_repeats() {
    let comm = Communicator::serial();
    let o = opts();
    for case in akantu_bench::suite_solid::cases() {
        let a = run_case(&case, &o, &comm, 0).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        if let Outcome::Skip(why) = &a.outcome {
            eprintln!("{}: skipped ({why})", case.name);
            continue;
        }
        let b = run_case(&case, &o, &comm, 1).unwrap();
        assert!(a.digest.is_some(), "{}: no digest", case.name);
        assert_eq!(
            a.digest, b.digest,
            "{}: digest differs between runs",
            case.name
        );
        assert!(!a.phases.is_empty(), "{}: no phases", case.name);
        let names_a: Vec<&str> = a.phases.iter().map(|p| p.phase.as_str()).collect();
        let names_b: Vec<&str> = b.phases.iter().map(|p| p.phase.as_str()).collect();
        assert_eq!(names_a, names_b, "{}: phase lists differ", case.name);
        assert!(
            a.counts[0] > 0 && a.counts[1] > 0,
            "{}: counts {:?}",
            case.name,
            a.counts
        );
        for p in &a.phases {
            assert!(
                p.seconds >= 0.0 && p.calls > 0,
                "{}: phase {:?}",
                case.name,
                p
            );
        }
    }
}

#[test]
fn case_names_are_unique() {
    let cases = akantu_bench::suite_solid::cases();
    let mut names: Vec<&str> = cases.iter().map(|c| c.name).collect();
    names.sort_unstable();
    let n = names.len();
    names.dedup();
    assert_eq!(names.len(), n);
}

/// `steps` steps with `solve_step` and with `solve_step_intercepted` give
/// bitwise identical states.
fn split_matches_plain(ty: ElementType, method: AnalysisMethod, dat: &str, dt_factor: f64) {
    let run = |split: bool| {
        let mesh = box_mesh(&BoxSpec::unit(ty, 2), None, true).unwrap();
        let mut m = common::solid_model(mesh, dat).unwrap();
        common::init(&mut m, method).unwrap();
        let dt = m.stable_time_step().unwrap();
        m.set_time_step(dt_factor * dt, None).unwrap();
        common::apply_pulse(&mut m, 0.0, 1.0).unwrap();
        let mut s = StepSplit {
            on: true,
            ..StepSplit::default()
        };
        for _ in 0..5 {
            if split {
                m.solve_step_intercepted(&mut s, None).unwrap();
            } else {
                m.solve_step(None).unwrap();
            }
        }
        common::digest(&m).unwrap()
    };
    assert_eq!(run(false), run(true), "{ty} {method:?}");
}

#[test]
fn step_split_computes_the_same_bits() {
    split_matches_plain(
        ElementType::Tetrahedron4,
        AnalysisMethod::ExplicitLumpedMass,
        &common::elastic_dat(false),
        0.8,
    );
    split_matches_plain(
        ElementType::Hexahedron8,
        AnalysisMethod::ExplicitLumpedMass,
        &common::elastic_dat(true),
        0.8,
    );
    split_matches_plain(
        ElementType::Tetrahedron4,
        AnalysisMethod::ImplicitDynamic,
        &common::elastic_dat(false),
        4.0,
    );
    split_matches_plain(
        ElementType::Tetrahedron4,
        AnalysisMethod::ExplicitLumpedMass,
        &akantu_bench::suite_solid::explicit::plastic_dat(),
        0.8,
    );
}

/// `static_linear_mumps` skips on the serial communicator of the test
/// above; the binary runs it on the MPI world (a singleton here), twice.
#[cfg(feature = "mumps")]
#[test]
fn mumps_case_runs_on_the_mpi_world_and_its_digest_repeats() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_suite_solid"))
        .args([
            "--case",
            "static_linear_mumps",
            "--size",
            "s",
            "--reps",
            "2",
        ])
        .output()
        .expect("suite_solid runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "suite_solid failed:\n{stdout}\n{stderr}"
    );
    let mut digests = [Vec::new(), Vec::new()];
    for line in stdout.lines() {
        let v: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}: {e}"));
        assert_eq!(v["sparse_solver"], "mumps", "{line}");
        let rep = v["rep"].as_u64().expect("rep") as usize;
        digests[rep].push(v["digest"].as_str().expect("a digest").to_owned());
    }
    assert!(!digests[0].is_empty(), "no output:\n{stderr}");
    assert_eq!(digests[0], digests[1], "digest differs between repetitions");
}
