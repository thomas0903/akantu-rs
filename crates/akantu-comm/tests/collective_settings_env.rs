// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the process settings of the collective protocol (D-126).
//
// An invalid `AKANTU_COLLECTIVE_CHECKS` (and, with MPI, an invalid
// `AKANTU_COLLECTIVE_HEADER`) is reported as an error, not taken for the
// default. The settings are read once per process: each case re-runs this
// test binary with the variable set and checks the child's verdict.

use std::process::Command;

/// Run the test `name` of this binary in a child process with `vars` set.
fn child(name: &str, vars: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(std::env::current_exe().expect("test binary"));
    cmd.args(["--exact", name, "--test-threads", "1"])
        .env("AKANTU_SETTINGS_CHILD", "1")
        .env_remove("AKANTU_COLLECTIVE_CHECKS")
        .env_remove("AKANTU_COLLECTIVE_HEADER");
    for (k, v) in vars {
        cmd.env(k, v);
    }
    cmd.output().expect("child test")
}

fn in_child() -> bool {
    std::env::var_os("AKANTU_SETTINGS_CHILD").is_some()
}

/// Child side (passes at once outside the child): the default mode is an
/// error naming the variable.
#[test]
fn child_invalid_collective_checks() {
    if !in_child() {
        return;
    }
    let e = akantu_comm::phase::default_collective_checks().unwrap_err();
    assert!(e.to_string().contains("AKANTU_COLLECTIVE_CHECKS"), "{e}");
    #[cfg(feature = "mpi")]
    {
        let w = akantu_comm::Communicator::mpi_world(akantu_comm::MpiOptions::default());
        let e = w.expect_err("mpi_world must refuse the setting");
        assert!(e.to_string().contains("AKANTU_COLLECTIVE_CHECKS"), "{e}");
    }
}

/// Child side (passes at once outside the child): a valid value is taken.
#[test]
fn child_valid_collective_checks() {
    if !in_child() {
        return;
    }
    assert_eq!(
        akantu_comm::phase::default_collective_checks().unwrap(),
        akantu_comm::phase::CollectiveChecks::EveryPhase
    );
}

/// Child side, with MPI (passes at once outside the child): an invalid
/// header form is an error of the world's creation.
#[cfg(feature = "mpi")]
#[test]
fn child_invalid_collective_header() {
    if !in_child() {
        return;
    }
    let e = akantu_comm::mpi_backend::header_exchange().unwrap_err();
    assert!(e.to_string().contains("AKANTU_COLLECTIVE_HEADER"), "{e}");
    let w = akantu_comm::Communicator::mpi_world(akantu_comm::MpiOptions::default());
    let e = w.expect_err("mpi_world must refuse the setting");
    assert!(e.to_string().contains("AKANTU_COLLECTIVE_HEADER"), "{e}");
}

fn passed(out: &std::process::Output, name: &str) {
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && text.contains("1 passed"),
        "{name}: {text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn invalid_collective_checks_is_an_error() {
    if in_child() {
        return;
    }
    let out = child(
        "child_invalid_collective_checks",
        &[("AKANTU_COLLECTIVE_CHECKS", "every-other-phase")],
    );
    passed(&out, "invalid AKANTU_COLLECTIVE_CHECKS");
}

#[test]
fn valid_collective_checks_is_taken() {
    if in_child() {
        return;
    }
    let out = child(
        "child_valid_collective_checks",
        &[("AKANTU_COLLECTIVE_CHECKS", " Every-Phase ")],
    );
    passed(&out, "valid AKANTU_COLLECTIVE_CHECKS");
}

#[cfg(feature = "mpi")]
#[test]
fn invalid_collective_header_is_an_error() {
    if in_child() {
        return;
    }
    let out = child(
        "child_invalid_collective_header",
        &[("AKANTU_COLLECTIVE_HEADER", "ring")],
    );
    passed(&out, "invalid AKANTU_COLLECTIVE_HEADER");
}
