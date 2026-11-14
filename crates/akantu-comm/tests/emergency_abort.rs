// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression (no upstream counterpart): the
// transport-failure abort path must terminate the process even when writing
// its diagnostic to stderr fails (`eprintln!` panics on EPIPE, and a panic
// unwinding out of the abort path leaves a worker's process running).

//! The test re-runs this binary as a child whose stderr is a pipe with no
//! reader (`EPIPE` on every write; Rust ignores `SIGPIPE`). The child calls
//! [`Communicator::abort_on_transport_fatal`] with a transport error; it
//! must exit with [`TRANSPORT_ABORT_CODE`] (before the fix the `eprintln!`
//! panicked instead: exit 101).

use std::io::Write as _;
use std::process::{Command, Stdio};

use akantu_comm::{Communicator, TRANSPORT_ABORT_CODE};
use akantu_core::{Error, FatalKind};

const CHILD: &str = "AKANTU_EMERGENCY_ABORT_CHILD";

#[test]
fn transport_abort_survives_a_broken_stderr() {
    let (reader, writer) = std::io::pipe().expect("pipe");
    let mut child = Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "child_aborts_with_broken_stderr",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(writer)
        .spawn()
        .expect("spawn child");
    drop(reader); // no reader left: the child's stderr writes fail (EPIPE)
    let status = child.wait().expect("child status");
    assert_eq!(
        status.code(),
        Some(TRANSPORT_ABORT_CODE),
        "the abort path did not terminate with the transport abort code: {status:?}"
    );
}

/// Child half (does nothing unless started by the test above).
#[test]
fn child_aborts_with_broken_stderr() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    // Wait until the parent closed the reader: a write fails (EPIPE).
    for _ in 0..10_000 {
        if std::io::stderr().write_all(b"probe\n").is_err() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let e = Error::fatal(
        FatalKind::Transport,
        Error::Communication("injected transport failure".into()),
    );
    Communicator::serial().abort_on_transport_fatal(&e);
    std::process::exit(3); // not reached: the abort must exit with 71
}
