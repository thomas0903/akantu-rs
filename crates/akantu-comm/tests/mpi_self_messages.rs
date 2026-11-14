// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression scenarios (no upstream counterpart): Open MPI
// 5.0.7 writes the whole message past a posted receive
// buffer when a self-message of 4 KiB or more, or an inter-process message
// of 64 KiB or more over TCP, is truncated
// (docs/DEPENDENCIES.md). Semantics follow
// src/synchronizer/communicator_mpi_inline_impl.hh (asyncReceive / send /
// probe with the own rank as peer).

//! Real-MPI regressions for messages to self and truncated posted receives,
//! one scenario per process (`harness = false`), launched by
//! `scripts/mpi/run-mpi-tier.sh` as
//!
//! ```text
//! mpirun [--mca btl tcp,self] -np N mpi_self_messages <scenario>
//! ```
//!
//! The binary runs under a **red-zone allocator** (the stable toolchain has
//! no AddressSanitizer): every heap block gets guard bytes before and after
//! it, checked when the block is freed; a damaged guard aborts the process
//! with `REDZONE VIOLATION`. A posted receive whose buffer MPI overruns is
//! freed right after its truncated completion, so the overrun of the
//! pinned Open MPI (4094 / 65534 / 1048574 bytes past a 2-byte buffer for
//! 4 KiB / 64 KiB / 1 MiB self-messages; 65534 / 1048574 for 64 KiB / 1 MiB
//! inter-process messages over TCP) is detected deterministically — or the
//! process dies earlier on the damaged heap; either way the job fails. Before
//! the fix (posted receives through a fixed-size `MPI_Irecv`) every
//! `self_*` scenario fails, and `cross_64k`/`cross_1m` fail over TCP.
//!
//! Each rank prints `SELF PASS <scenario> rank=<r>` (or `SELF FAIL …`) and
//! the job must exit 0. The `cross_*` scenarios need two ranks (PASS, not
//! applicable, on one). Without an argument the scenarios are listed (exit
//! 0, e.g. under `cargo test`).

#![allow(unsafe_code)] // test-only global allocator (red zones); every block states its SAFETY.

mod mpi_common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, SendMode, Tag};
use mpi_common::{op_timeout, start_watchdog, world};

// ------------------------------------------------------- red-zone allocator

/// Guard bytes before and after every block.
const RED: usize = 64;
const GUARD: u8 = 0xA5;

/// System allocator with guard bytes around every block, checked on free.
struct RedZone;

/// Offset of the user block inside the underlying allocation (a multiple
/// of the alignment, at least `RED`).
fn front(align: usize) -> usize {
    RED.max(align)
}

/// Underlying layout for a user layout (`None` on overflow).
fn outer(layout: Layout) -> Option<(Layout, usize)> {
    let align = layout.align().max(16);
    let f = front(align);
    let total = f.checked_add(layout.size())?.checked_add(RED)?;
    Layout::from_size_align(total, align).ok().map(|l| (l, f))
}

unsafe extern "C" {
    fn write(fd: i32, buf: *const std::ffi::c_void, count: usize) -> isize;
    fn abort() -> !;
}

fn violation(msg: &[u8]) -> ! {
    // SAFETY: `msg` is a valid slice; `write` and `abort` are async-signal
    // safe, allocate nothing and have no other preconditions.
    unsafe {
        let _ = write(2, msg.as_ptr().cast(), msg.len());
        abort()
    }
}

// SAFETY: forwards to `System` with a larger layout of the same (or
// larger) alignment; the user pointer lies `front` bytes inside the
// underlying block, which is a multiple of the alignment, and `size + RED`
// bytes of the underlying block follow it. `dealloc` recomputes the same
// outer layout from the user layout (GlobalAlloc's contract: same layout).
unsafe impl GlobalAlloc for RedZone {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some((o, f)) = outer(layout) else {
            return std::ptr::null_mut();
        };
        // SAFETY: `o` has a non-zero size (at least 2 × RED).
        let base = unsafe { System.alloc(o) };
        if base.is_null() {
            return base;
        }
        // SAFETY: `base` points to `o.size() = f + size + RED` writable
        // bytes: the front guard `[0, f)`, the user block `[f, f + size)`
        // and the back guard `[f + size, o.size())`.
        unsafe {
            std::ptr::write_bytes(base, GUARD, f);
            std::ptr::write_bytes(base.add(f + layout.size()), GUARD, RED);
            base.add(f)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let Some((o, f)) = outer(layout) else {
            violation(b"REDZONE VIOLATION: dealloc with an invalid layout\n");
        };
        // SAFETY: `ptr` was returned by `alloc` with this layout, so the
        // underlying block starts `f` bytes before it and has `o.size()`
        // readable bytes.
        unsafe {
            let base = ptr.sub(f);
            let head = std::slice::from_raw_parts(base, f);
            let tail = std::slice::from_raw_parts(ptr.add(layout.size()), RED);
            if head.iter().any(|&b| b != GUARD) {
                violation(b"REDZONE VIOLATION: bytes written before a heap block\n");
            }
            if tail.iter().any(|&b| b != GUARD) {
                violation(b"REDZONE VIOLATION: bytes written past the end of a heap block\n");
            }
            System.dealloc(base, o);
        }
    }
}

#[global_allocator]
static ALLOC: RedZone = RedZone;

// ---------------------------------------------------------------- scenarios

const SCENARIOS: &[&str] = &[
    "redzone_selftest",
    "self_4k",
    "self_64k",
    "self_1m",
    "cross_4k",
    "cross_64k",
    "cross_1m",
    "self_semantics",
];

type Outcome = Result<(), String>;

fn e2s(e: akantu_core::Error) -> String {
    e.to_string()
}

fn pattern(i: usize, salt: u8) -> u8 {
    u8::try_from(i % 251).unwrap_or(0) ^ salt
}

fn payload(len: usize, salt: u8) -> Vec<u8> {
    (0..len).map(|i| pattern(i, salt)).collect()
}

/// `r` is the ordinary (non-transport) truncation error.
fn is_truncation(r: &Result<Vec<u8>, akantu_core::Error>) -> bool {
    r.as_ref()
        .is_err_and(|e| e.fatal_kind().is_none() && e.to_string().contains("truncated"))
}

/// The detector works: a heap block overrun by one byte aborts the process
/// when freed. Run in a child process (`redzone_selftest_child`, which must
/// die with SIGABRT); never under mpirun's other scenarios.
fn redzone_selftest() -> Outcome {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = std::process::Command::new(exe)
        .arg("redzone_selftest_child")
        .output()
        .map_err(|e| e.to_string())?;
    let err = String::from_utf8_lossy(&out.stderr);
    if out.status.success() || !err.contains("REDZONE VIOLATION: bytes written past the end") {
        return Err(format!(
            "the red-zone allocator missed a one-byte overrun (status {:?}, stderr {err:?})",
            out.status
        ));
    }
    Ok(())
}

fn redzone_selftest_child() -> ! {
    let mut v = vec![0u8; 5];
    let p = v.as_mut_ptr();
    // SAFETY: deliberately **not** sound — writes one byte past the
    // 5-byte block into its back guard (still inside the underlying
    // allocation of this allocator), to prove that the guard check fires.
    unsafe { p.add(5).write_volatile(0x55) };
    drop(v);
    eprintln!("red-zone overrun not detected");
    std::process::exit(0)
}

/// A self-message of `len` bytes into a 4-byte posted receive is an
/// ordinary truncation error (no byte written past any buffer); the same
/// message into an exact posted receive, a matched receive and a
/// synchronous send arrive intact; the communicator stays healthy.
fn self_truncation(c: &Communicator, len: usize) -> Outcome {
    let me = c.rank();
    let msg = payload(len, 0x55);
    // Truncated (the original reproducer).
    let rq = c.irecv::<u8>(4, me, Tag(9)).map_err(e2s)?;
    let s = c.isend(msg.clone(), me, Tag(9)).map_err(e2s)?;
    let r = rq.wait_received();
    if !is_truncation(&r) {
        return Err(format!("{len} bytes into 4: {r:?}"));
    }
    s.wait().map_err(e2s)?;
    // Posted before and after the send, exact length.
    let rq = c.irecv::<u8>(len, me, Tag(10)).map_err(e2s)?;
    c.send(&msg, me, Tag(10)).map_err(e2s)?;
    if rq.wait().map_err(e2s)? != msg {
        return Err(format!("{len} bytes: posted receive got a wrong payload"));
    }
    let s = c.isend(msg.clone(), me, Tag(11)).map_err(e2s)?;
    let rq = c.irecv::<u8>(len, me, Tag(11)).map_err(e2s)?;
    if rq.wait_received().map_err(e2s)? != msg {
        return Err(format!(
            "{len} bytes: late posted receive got a wrong payload"
        ));
    }
    s.wait().map_err(e2s)?;
    // Matched receive (probe for the size first, as upstream does).
    let s = c
        .isend_mode(msg.clone(), me, Tag(12), SendMode::Rendezvous)
        .map_err(e2s)?;
    let st = c.probe::<u8>(Some(me), Tag(12)).map_err(e2s)?;
    if st.source != me || st.count != len {
        return Err(format!("probe: {st:?}"));
    }
    if c.receive_vec::<u8>(me, Tag(12)).map_err(e2s)? != msg {
        return Err(format!("{len} bytes: matched receive got a wrong payload"));
    }
    s.wait().map_err(e2s)?;
    // Several truncations in a row, then the communicator still works.
    for k in 0..3u8 {
        let rq = c
            .irecv::<u8>(1 + usize::from(k), me, Tag(13))
            .map_err(e2s)?;
        c.send(&msg, me, Tag(13)).map_err(e2s)?;
        let r = rq.wait_received();
        if !is_truncation(&r) {
            return Err(format!("repeat {k}: {r:?}"));
        }
    }
    c.barrier().map_err(e2s)?;
    if let Some(b) = c.broken() {
        return Err(format!("communicator broken: {b}"));
    }
    Ok(())
}

/// Inter-process truncation of `len` bytes into a 4-byte posted receive:
/// an ordinary error and no overrun (MPI receives the matched message at
/// its exact length; a fixed-size `MPI_Irecv` is overrun over TCP from
/// 64 KiB on). Ring: rank r receives from its predecessor.
fn cross_truncation(c: &Communicator, len: usize) -> Outcome {
    let (r, p) = (c.rank(), c.size());
    let pi = i32::try_from(p).map_err(|e| e.to_string())?;
    let (next, prev) = ((r + 1) % pi, (r + pi - 1) % pi);
    let msg = payload(len, 0x5A);
    for mode in [SendMode::Eager, SendMode::Rendezvous] {
        let rq = c.irecv::<u8>(4, prev, Tag(20)).map_err(e2s)?;
        let s = c
            .isend_mode(msg.clone(), next, Tag(20), mode)
            .map_err(e2s)?;
        let got = rq.wait_received();
        if !is_truncation(&got) {
            return Err(format!("{len} bytes into 4 ({mode:?}): {got:?}"));
        }
        s.wait().map_err(e2s)?;
        let rq = c.irecv::<u8>(len, prev, Tag(21)).map_err(e2s)?;
        c.send_mode(&msg, next, Tag(21), mode).map_err(e2s)?;
        if rq.wait().map_err(e2s)? != msg {
            return Err(format!("{len} bytes ({mode:?}): wrong payload"));
        }
    }
    c.barrier().map_err(e2s)?;
    if let Some(b) = c.broken() {
        return Err(format!("communicator broken: {b}"));
    }
    Ok(())
}

/// MPI semantics of the self channel with messages beyond the eager limit:
/// posting order whatever the wait order, synchronous completion only once
/// matched (`test`), a synchronous send completed by a pre-posted receive,
/// any-source probes that see self-messages, cancellation of a dropped
/// unmatched receive and discard of a matched one.
fn self_semantics(c: &Communicator) -> Outcome {
    let me = c.rank();
    let (a_msg, b_msg) = (payload(8 << 10, 1), payload(70 << 10, 2));
    // Posting order (both messages longer than the eager limit).
    let a = c.irecv::<u8>(80 << 10, me, Tag(30)).map_err(e2s)?;
    let b = c.irecv::<u8>(80 << 10, me, Tag(30)).map_err(e2s)?;
    let s1 = c.isend(a_msg.clone(), me, Tag(30)).map_err(e2s)?;
    let s2 = c.isend(b_msg.clone(), me, Tag(30)).map_err(e2s)?;
    let second = b.wait_received().map_err(e2s)?;
    let first = a.wait_received().map_err(e2s)?;
    s1.wait().map_err(e2s)?;
    s2.wait().map_err(e2s)?;
    if first != a_msg || second != b_msg {
        return Err("posting order violated for self-messages".into());
    }
    // A synchronous self-send completes only when a receive matches it.
    let mut s = c
        .isend_mode(a_msg.clone(), me, Tag(31), SendMode::Rendezvous)
        .map_err(e2s)?;
    if s.test().map_err(e2s)? {
        return Err("synchronous self-send complete before any receive".into());
    }
    // Any-source probe sees the self-message.
    let st = c
        .iprobe::<u8>(None, Tag(31))
        .map_err(e2s)?
        .ok_or("any-source iprobe missed the self-message")?;
    if st.source != me || st.count != a_msg.len() {
        return Err(format!("any-source iprobe: {st:?}"));
    }
    if c.receive_vec::<u8>(me, Tag(31)).map_err(e2s)? != a_msg {
        return Err("matched receive of the synchronous self-send".into());
    }
    if !s.test().map_err(e2s)? {
        return Err("synchronous self-send not complete after the receive".into());
    }
    s.wait().map_err(e2s)?;
    // Blocking synchronous self-send completed by a pre-posted receive.
    let rq = c.irecv::<u8>(b_msg.len(), me, Tag(32)).map_err(e2s)?;
    let t0 = Instant::now();
    c.send_mode(&b_msg, me, Tag(32), SendMode::Rendezvous)
        .map_err(e2s)?;
    if t0.elapsed() > Duration::from_secs(2) {
        return Err("synchronous self-send waited for a deadline".into());
    }
    if rq.wait().map_err(e2s)? != b_msg {
        return Err("pre-posted receive of a synchronous self-send".into());
    }
    // Dropped unmatched receive: withdrawn. (Non-blocking send: a blocking
    // standard send of a large message to self before its receive is
    // posted relies on buffering, which MPI does not guarantee.)
    drop(c.irecv::<u8>(4, me, Tag(33)).map_err(e2s)?);
    let s = c.isend(a_msg.clone(), me, Tag(33)).map_err(e2s)?;
    if c.receive_vec::<u8>(me, Tag(33)).map_err(e2s)? != a_msg {
        return Err("a dropped receive swallowed the next self-message".into());
    }
    s.wait().map_err(e2s)?;
    // Dropped matched receive: message discarded.
    let matched = c.irecv::<u8>(a_msg.len(), me, Tag(34)).map_err(e2s)?;
    c.send(&a_msg, me, Tag(34)).map_err(e2s)?;
    drop(matched);
    if c.iprobe::<u8>(Some(me), Tag(34)).map_err(e2s)?.is_some() {
        return Err("a matched self-message survived its dropped receive".into());
    }
    c.barrier().map_err(e2s)?;
    if let Some(b) = c.broken() {
        return Err(format!("communicator broken: {b}"));
    }
    Ok(())
}

fn main() {
    let arg = std::env::args().nth(1);
    if arg.as_deref() == Some("redzone_selftest_child") {
        redzone_selftest_child();
    }
    let Some(name) = arg.filter(|a| SCENARIOS.contains(&a.as_str())) else {
        println!("mpi_self_messages scenarios (under the red-zone allocator):");
        for s in SCENARIOS {
            println!("  {s}");
        }
        return;
    };
    start_watchdog(120);
    if name == "redzone_selftest" {
        let r = redzone_selftest();
        match &r {
            Ok(()) => println!("SELF PASS {name} rank=0"),
            Err(e) => println!("SELF FAIL {name} rank=0: {e}"),
        }
        std::process::exit(i32::from(r.is_err()));
    }
    let c = world(ThreadLevel::Funneled, Some(op_timeout()), SendMode::Eager);
    let size = |s: &str| match s {
        "4k" => 4 << 10,
        "64k" => 64 << 10,
        _ => 1 << 20,
    };
    let r = match name.split_once('_') {
        Some(("self", "semantics")) => self_semantics(&c),
        Some(("self", s)) => self_truncation(&c, size(s)),
        Some(("cross", _)) if c.size() == 1 => {
            println!("INFO {name}: not applicable on one rank");
            Ok(())
        }
        Some(("cross", s)) => cross_truncation(&c, size(s)),
        _ => Err(format!("unknown scenario {name}")),
    };
    match &r {
        Ok(()) => println!("SELF PASS {name} rank={}", c.rank()),
        Err(e) => println!("SELF FAIL {name} rank={}: {e}", c.rank()),
    }
    let code = i32::from(r.is_err());
    drop(c); // last guard on the main thread: MPI_Finalize
    std::process::exit(code);
}
