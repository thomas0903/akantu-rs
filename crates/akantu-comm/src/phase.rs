// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs design (no direct upstream counterpart): upstream lets a rank
// throw while its peers wait in a collective (e.g. between the phases of
// GlobalIdsUpdater / NodeSynchronizer::onNodesAdded); this module defines the
// collective error protocol that replaces it (D-018).

//! Collective phases and matched preflight agreements.
//!
//! A distributed operation is a sequence of **named phases**. Each phase is
//!
//! 1. **preflight** — purely local validation (inputs, sizes, capacity
//!    limits, backend index widths); no communication, no mutation of shared
//!    state;
//! 2. **agreement** — [`agree`]: one bounded all-reduce of
//!    `(status, phase id)`; every rank, including ranks with nothing to do,
//!    participates. A rank whose preflight failed still joins the agreement,
//!    so no peer can be left waiting for a message that will never be posted;
//! 3. **communication** — posted only if the agreement succeeded on all
//!    ranks; failures here are [`Severity::Fatal`] (the communicator state is
//!    unknown), outstanding owned requests are completed before returning;
//! 4. **post-check** — optional second agreement when local
//!    post-processing can fail (e.g. consumer updates of a mesh mutation),
//!    after which a failure poisons the owning object on **all** ranks.
//!
//! The phase id is part of the agreement: ranks that reach different phases
//! (a protocol bug) get an error instead of a mismatched collective.
//! Recoverable local errors keep the caller's state unchanged (preflight
//! never mutates); fatal errors must be escalated (poison / abort from the
//! top-level runtime only).
//!
//! # The phase API: [`Phases`]
//!
//! Code that communicates *inside* a larger collective operation (mesh
//! mutation hooks, mesh-event consumers such as the DOF manager,
//! synchronizer exchanges) receives a [`Phases`] scope instead of an
//! unconstrained communicator, and performs every communication step
//! through [`Phases::run`] (`prepared` local result → agreement →
//! communication → post agreement) or [`Phases::collective`] (a matched
//! collective whose participation was already agreed). The scope enforces
//! the invariant that makes rank-specific failures bounded:
//!
//! * an agreement result is **common knowledge**: every rank of the
//!   all-reduce receives the same reduced status, so after a failed
//!   agreement every rank knows the operation failed at the same step;
//! * after a failed agreement or a transport failure the scope is
//!   [`Phases::is_failed`]: further `agree`/`run`/`collective` calls return
//!   an error **without communicating** — the owner's own post agreement
//!   (e.g. the mesh dispatcher's) only classifies the rank's result — so
//!   ranks that detected the failure in different code paths (one in a
//!   consumer's internal phase, one in the dispatcher's post agreement)
//!   never wait for each other again;
//! * every agreement id includes the scope name and the scope's agreement
//!   counter, so ranks at different steps — even with the same phase name —
//!   fail on all ranks instead of mismatching collectives;
//! * a failure *inside* a communication step ([`Phases::run`]'s outer
//!   `Err`) is a transport failure: the scope becomes
//!   [`Phases::is_broken`] (communicator state unknown, `Severity::Fatal`);
//!   local post-processing failures (the inner `Err`) are agreed by the
//!   post agreement after all communication completed.
//!
//! A local failure detected **before** a rank's next agreement is reported
//! through that agreement; a code path must therefore never start a raw
//! collective or point-to-point exchange without a preceding agreement of
//! the same scope (the local world detects a violation as an operation-code
//! mismatch; MPI would hang).
//!
//! # One live owner scope per communicator
//!
//! A live [`Phases`] is registered on its communicator (shared by all
//! clones of a rank's handle) and owned by one thread. While it is live, a
//! second, independent scope on the same communicator — from **any**
//! thread — is **refused** ([`Phases::is_refused`]: it never communicates
//! and its operations fail locally), and raw communicator operations fail
//! locally unless issued by the owner thread inside one of the scope's
//! communication steps. Both are ordinary errors that the enclosing scope's
//! next agreement reports on every rank; nested code communicates through
//! the scope it was handed. A failure reported by another rank at a
//! different step is reported as "failed on another rank" (the protocol
//! working), a step mismatch with every rank reporting success as a
//! protocol divergence (fatal).
//!
//! **Thread confinement.** A live `Phases` is `!Send + !Sync`: it
//! cannot be moved or lent to another thread, so its registration (the
//! owner thread consulted by admission checks) always matches the thread
//! using it. Lending a live scope to a worker does not compile: the
//! reproduction below is rejected with `E0277` (`*const ()`
//! cannot be sent between threads safely):
//!
//! ```compile_fail,E0277
//! # use std::time::Duration;
//! # use akantu_comm::run_local;
//! # use akantu_comm::phase::{Phases, Severity};
//! # use akantu_core::Error;
//! run_local(2, Duration::from_millis(250), |c| {
//!     let mut outer = Phases::new(&c, "outer");
//!     std::thread::scope(|s| {
//!         s.spawn(|| {
//!             let local = if c.rank() == 0 {
//!                 Err(Error::invalid("local callback failure"))
//!             } else {
//!                 let mut nested = Phases::new(&c, "independent-child");
//!                 nested.agree("child", Ok(()), Severity::Recoverable)
//!             };
//!             outer.agree("callback:done", local, Severity::Recoverable)
//!         })
//!         .join()
//!         .unwrap()
//!     })
//! });
//! ```
//!
//! The one permitted transfer is explicit and checked: [`Phases::park`]
//! yields a `Send` [`ParkedScope`] that stays registered (admission keeps
//! refusing other scopes and raw operations on every thread while it is in
//! transit), and [`ParkedScope::resume`] re-registers the resuming thread
//! as the owner. The scope's step counter and failure state travel with
//! it, so there is always exactly one effective owner.
//!
//! # Agreed severity
//!
//! A failed scope keeps the agreed worst severity, identical on every rank
//! ([`Phases::is_fatal`], [`Phases::failure`]); owners turn a fatal or
//! broken scope into `akantu_core::Error::Fatal` ([`Phases::settle`]) and
//! poison themselves. The severity a rank reports is derived from its
//! local outcome **centrally**, whatever the caller passed: a local
//! `Error::Fatal { kind: Agreed }` (a callback propagating an unusable
//! dependent owner's error) is reported as [`Severity::Fatal`], so every
//! rank's scope becomes fatal; a local `Error::Fatal { kind: Transport }`
//! is never reported at all — the scope breaks and the communicator is
//! marked broken without communicating (the abort path: no further
//! agreement on it, from any scope). A fatal outcome that can no longer be
//! agreed (the scope already failed recoverably) also takes the abort path.
//! Owners therefore pass **every** late result — a callback's, hook's,
//! consumer's or native driver's outcome returned after an internal phase
//! failed — through the scope's agreement, which classifies it without
//! communicating on a failed scope; an outcome combining
//! several errors keeps the most urgent fatal kind
//! (`akantu_core::Error::escalated_by`).

use std::marker::PhantomData;

use akantu_core::{Error, FatalKind, Result};

use crate::communicator::{
    Communicator, HeaderStatus, OpenRefusal, ReduceOp, arm_status, disarm_status, take_verdict,
};

/// Environment variable of the process default [`CollectiveChecks`].
pub const ENV_COLLECTIVE_CHECKS: &str = "AKANTU_COLLECTIVE_CHECKS";

/// Where the collective error protocol spends an agreement (D-126).
///
/// Both modes give the same guarantees: every rank ends an operation that
/// failed on any rank with an error of the same agreed severity class, no
/// rank waits for a peer that left the operation (the polled waits and
/// their deadlines stay the last line of defence), and no rank commits what
/// another rank rolled back. They differ in **when** a peer's purely local
/// failure becomes visible:
///
/// * [`CollectiveChecks::EveryPhase`] (D-018 as written): at the agreement
///   that ends the failed phase — every phase is followed by one;
/// * [`CollectiveChecks::CommitPoints`] (default): at the scope's next
///   agreement — before its next communication (a checked reduction carries
///   the pending outcomes in its own header, see [`Phases::run_collective`]),
///   at an explicit commit point ([`Phases::commit`]) or when the scope
///   settles. Local phases in between ([`Phases::defer`]) run on the other
///   ranks first; the failed rank skips them and keeps its own error.
///
/// The mode belongs to a communicator family (duplicates and splits inherit
/// it) and must be the same on every rank: a rank in another mode issues its
/// agreements at other points. New communicators take the process default
/// ([`set_default_collective_checks`], else `AKANTU_COLLECTIVE_CHECKS` =
/// `every-phase` | `commit-points`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CollectiveChecks {
    /// An agreement after every phase.
    EveryPhase,
    /// Agreements only where the ranks decide together or are about to
    /// depend on each other.
    #[default]
    CommitPoints,
}

impl CollectiveChecks {
    /// The name used by `AKANTU_COLLECTIVE_CHECKS`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::EveryPhase => "every-phase",
            Self::CommitPoints => "commit-points",
        }
    }

    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::EveryPhase => 1,
            Self::CommitPoints => 2,
        }
    }

    pub(crate) const fn from_code(c: u8) -> Self {
        if c == 1 {
            Self::EveryPhase
        } else {
            Self::CommitPoints
        }
    }
}

impl std::str::FromStr for CollectiveChecks {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "every-phase" => Ok(Self::EveryPhase),
            "commit-points" => Ok(Self::CommitPoints),
            other => Err(format!(
                "collective checks `{other}`: expected every-phase or commit-points"
            )),
        }
    }
}

/// Process default (0: not read yet).
static DEFAULT_CHECKS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The mode new communicator families start with: the one given to
/// [`set_default_collective_checks`], else `AKANTU_COLLECTIVE_CHECKS` (read
/// once; empty or unset: [`CollectiveChecks::CommitPoints`]).
///
/// # Errors
/// An invalid `AKANTU_COLLECTIVE_CHECKS` (the error repeats on every call).
pub fn default_collective_checks() -> Result<CollectiveChecks> {
    use std::sync::atomic::Ordering::Relaxed;
    let c = DEFAULT_CHECKS.load(Relaxed);
    if c != 0 {
        return Ok(CollectiveChecks::from_code(c));
    }
    let mode = match std::env::var(ENV_COLLECTIVE_CHECKS) {
        Ok(v) if !v.trim().is_empty() => v
            .parse::<CollectiveChecks>()
            .map_err(|e| Error::invalid(format!("{ENV_COLLECTIVE_CHECKS}: {e}")))?,
        _ => CollectiveChecks::default(),
    };
    DEFAULT_CHECKS.store(mode.code(), Relaxed);
    Ok(mode)
}

/// Set the mode of communicator families created from now on (programs;
/// tests set it on their own communicators,
/// `Communicator::set_collective_checks`). It must be the same on every
/// rank.
pub fn set_default_collective_checks(mode: CollectiveChecks) {
    DEFAULT_CHECKS.store(mode.code(), std::sync::atomic::Ordering::Relaxed);
}

/// Failure class of a phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Everything fine.
    Ok = 0,
    /// Local validation failed before any communication: state unchanged on
    /// every rank, the operation can be retried after fixing the input.
    Recoverable = 1,
    /// A failure after communication started or during post-processing:
    /// the owning object is poisoned on every rank.
    Fatal = 2,
}

/// Stable 64-bit id of a phase name (FNV-1a), identical on every rank.
#[must_use]
pub fn phase_id(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// FNV-1a continuation: fold `bytes` into digest `h` (for schedule
/// descriptors agreed with [`agree_digest`]).
#[must_use]
pub fn digest_bytes(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Outcome of one status all-reduce (transport errors are the outer `Err`).
struct Reduced {
    worst: u64,
    ids_match: bool,
    digests_match: bool,
}

/// Severity a rank reports for its local outcome: `Ok` for
/// success; [`Severity::Fatal`] for a local `Error::Fatal`, whatever the
/// caller passed; otherwise the caller's severity, at least
/// [`Severity::Recoverable`].
fn reported_severity(local: &Result<()>, severity: Severity) -> Severity {
    match local {
        Ok(()) => Severity::Ok,
        Err(e) if e.is_fatal() => Severity::Fatal,
        Err(_) => severity.max(Severity::Recoverable),
    }
}

/// `true` for a local transport-fatal outcome: never agreed.
fn is_transport(e: &Error) -> bool {
    e.fatal_kind() == Some(FatalKind::Transport)
}

/// The result of a **header-agreed raw collective** issued inside a
/// [`Phases::run`] communication step — `Communicator::native_collective`
/// — split into the step's `Result<Result<R>>`:
///
/// * a transport-fatal error (`Error::Fatal { kind: Transport }`: a failed
///   MPI call, a deadline) is the step's **outer** failure: the scope and
///   the communicator family break, as before;
/// * any other error was agreed on every rank by the collective's own
///   checked header before anything was handed to the library — a
///   `Serialized` admission refusal, ranks entering different native jobs,
///   a failed local validation or thread-policy check — so it is returned
///   as the **inner** result: the step's post agreement fails the scope
///   recoverably on every rank and the communicator stays usable
///   (before, `native_collective(.)?` inside the closure turned
///   an agreed refusal into a transport failure that broke the family).
///
/// Only for collectives whose non-transport errors are common knowledge
/// (inside a scope's communication step the raw-operation check of
/// `native_collective` always passes, and every later error comes from its
/// header). A library failure *inside* the native call is the caller's
/// value `R` and keeps its own classification.
pub fn agreed_or_transport<R>(r: Result<R>) -> Result<Result<R>> {
    match r {
        Ok(v) => Ok(Ok(v)),
        Err(e) if is_transport(&e) => Err(e),
        Err(e) => Ok(Err(e)),
    }
}

/// Agreements performed by this process ([`agreement_stats`]).
static AGREEMENTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Nanoseconds spent in them, waiting for the other ranks included.
static AGREEMENT_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Number of agreements (the all-reduce of every [`agree`],
/// [`Phases::agree`], [`Phases::run`] step, …) this process performed so
/// far, and the seconds spent in them, waiting for slower ranks included:
/// a measurement aid for the cost of the collective error protocol
/// (counts of every communicator of the process; with the in-process
/// world of [`crate::run_local`], of every rank together).
#[must_use]
pub fn agreement_stats() -> (u64, f64) {
    use std::sync::atomic::Ordering::Relaxed;
    #[allow(clippy::cast_precision_loss)] // a duration in ns
    let s = AGREEMENT_NANOS.load(Relaxed) as f64 * 1e-9;
    (AGREEMENTS.load(Relaxed), s)
}

/// A communicator of one rank on the serial or the in-process back-end:
/// its agreements need no all-reduce. An MPI communicator of one rank
/// still calls MPI, whose thread-level and transport checks an agreement
/// must observe (a `Funneled` call from a worker breaks the scope).
fn one_rank_without_mpi(comm: &Communicator) -> bool {
    comm.size() == 1 && !comm.is_mpi()
}

/// Count one agreement and, when it communicated, the time since `start`.
fn count_agreement(start: Option<std::time::Instant>) {
    use std::sync::atomic::Ordering::Relaxed;
    AGREEMENTS.fetch_add(1, Relaxed);
    if let Some(t) = start {
        let ns = u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX);
        AGREEMENT_NANOS.fetch_add(ns, Relaxed);
    }
}

fn reduce_status(comm: &Communicator, id: u64, status: Severity, digest: u64) -> Result<Reduced> {
    // max(status), max(id), max(!id) = !min(id), likewise for the digest:
    // one bounded all-reduce of five words.
    if one_rank_without_mpi(comm) {
        // one rank without MPI: the all-reduce returns the rank's own words
        count_agreement(None);
        return Ok(Reduced {
            worst: status as u64,
            ids_match: true,
            digests_match: true,
        });
    }
    // D-126: under commit points the agreement also carries its status in
    // the collective's header, so that it meets a checked reduction that
    // folds a peer's deferred outcomes (`Phases::run_collective`), or a
    // failed peer's agreement, with one verdict on every rank
    let armed = comm.collective_checks() == CollectiveChecks::CommitPoints;
    if armed {
        arm_status(
            comm,
            HeaderStatus {
                severity: status as u8,
                id,
                digest,
            },
        );
    }
    let mut v = [status as u64, id, !id, digest, !digest];
    let t = std::time::Instant::now();
    let r = comm.all_reduce(&mut v, ReduceOp::Max);
    count_agreement(Some(t));
    if armed {
        disarm_status();
        if let Some(verdict) = take_verdict() {
            return Ok(Reduced {
                worst: u64::from(verdict.worst),
                ids_match: verdict.ids_match,
                digests_match: verdict.digests_match,
            });
        }
    }
    r?;
    Ok(Reduced {
        worst: v[0],
        ids_match: v[1] == !v[2],
        digests_match: v[3] == !v[4],
    })
}

/// `true` while the current thread runs inside a native library's region
/// (MPI backend): such a library may communicate outside the protocol, so
/// nothing is deferred there (D-126).
fn in_native_region() -> bool {
    #[cfg(feature = "mpi")]
    {
        crate::mpi_backend::in_native_region()
    }
    #[cfg(not(feature = "mpi"))]
    {
        false
    }
}

/// Outcomes deferred since a scope's last agreement (D-126): this rank's
/// worst severity, the folded qualified names and digests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Deferred {
    worst: Severity,
    id: u64,
    digest: u64,
}

impl Deferred {
    /// Fold one more outcome (`q`: its qualified name).
    fn fold(prev: Option<Self>, q: &str, status: Severity, digest: u64) -> Self {
        match prev {
            None => Self {
                worst: status,
                id: phase_id(q),
                digest: digest_bytes(phase_id("digest"), &digest.to_le_bytes()),
            },
            Some(d) => Self {
                worst: d.worst.max(status),
                id: digest_bytes(d.id, q.as_bytes()),
                digest: digest_bytes(d.digest, &digest.to_le_bytes()),
            },
        }
    }
}

/// Deferred outcomes counted per process (measurement aid next to
/// [`agreement_stats`]).
static DEFERRED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Number of phase outcomes this process deferred to a later agreement
/// instead of agreeing them at once (D-126).
#[must_use]
pub fn deferred_stats() -> u64 {
    DEFERRED.load(std::sync::atomic::Ordering::Relaxed)
}

/// `true` if a failed agreement must be classified fatal on every rank:
/// a fatal status was reported, or the ranks are at different steps while
/// all reporting success (a protocol divergence).
fn agreed_fatal(r: &Reduced) -> bool {
    r.worst >= Severity::Fatal as u64 || (!r.ids_match && r.worst == Severity::Ok as u64)
}

fn interpret(name: &str, local: Result<()>, r: &Reduced, digest_what: &str) -> Result<()> {
    // Every rank fails whenever anything is wrong; a rank's own local error
    // takes precedence in its report (failed ranks' digests are moot). A
    // failure reported by another rank wins over a step mismatch: the
    // failing rank reports at its next agreement, which may be a different
    // step than the one this rank is at — that is the protocol
    // working, not a divergence. Only a mismatch with every rank reporting
    // success is a protocol divergence.
    match local {
        Err(e) => Err(e),
        Ok(()) if r.worst != Severity::Ok as u64 => Err(Error::Communication(format!(
            "phase '{name}' failed on another rank ({}{})",
            if r.worst >= Severity::Fatal as u64 {
                "fatal"
            } else {
                "recoverable"
            },
            if r.ids_match {
                ""
            } else {
                ", reported at a different step"
            }
        ))),
        Ok(()) if !r.ids_match => Err(Error::Communication(format!(
            "collective phase mismatch: this rank is in '{name}', another rank is in a different phase"
        ))),
        Ok(()) if !r.digests_match => Err(Error::Communication(format!(
            "phase '{name}': ranks disagree on the {digest_what}"
        ))),
        Ok(()) => Ok(()),
    }
}

/// Matched agreement on the outcome of phase `name`.
///
/// Returns `Ok(())` iff every rank passed `Ok`. Otherwise every rank
/// returns an error: its own error if it failed locally, else an
/// `Error::Communication` naming the phase and the worst remote severity.
/// Also fails on every rank if the ranks are in different phases. The
/// reported severity follows the central rule of the module docs (a local
/// `Error::Fatal` is reported fatal; a local transport-fatal error is not
/// reported at all and marks the communicator broken); an agreed fatal
/// failure is returned as `Error::Fatal { kind: Agreed }`, a transport
/// failure of the agreement itself as `Error::Fatal { kind: Transport }`
/// (the communicator is then broken). Refused, like a raw operation, while
/// a scope is live on the communicator (an ordinary error; a local error is
/// returned with its own class), and on a communicator that observed a
/// transport failure — then always as `Error::Fatal { kind: Transport }`,
/// keeping a local error's diagnostic.
pub fn agree(
    comm: &Communicator,
    name: &str,
    local: Result<()>,
    severity_on_error: Severity,
) -> Result<()> {
    agree_digest(comm, name, local, 0, severity_on_error)
}

/// [`agree`] that additionally checks that every rank passed the same
/// `digest` (a hash of a schedule/descriptor, e.g. the kinds and parts of
/// the events of a mesh mutation and its consumer dispatch order); a
/// mismatch fails on every rank.
pub fn agree_digest(
    comm: &Communicator,
    name: &str,
    local: Result<()>,
    digest: u64,
    severity_on_error: Severity,
) -> Result<()> {
    if let Err(e) = &local
        && is_transport(e)
    {
        comm.mark_broken(&format!("agreement '{name}': {e}"));
        return local;
    }
    if let Err(refused) = comm.check_raw("agreement") {
        // Nothing is communicated. No owner settles a free agreement, so
        // the admission failure and the local outcome are combined here
        // with fatal priority: on a broken communicator the
        // refusal is transport-fatal and wins over an ordinary or
        // agreed-kind local error (whose diagnostic is kept); a refusal by
        // a live scope on a healthy communicator is ordinary and leaves the
        // local error's class unchanged (an agreed-kind error is passed on
        // to the enclosing scope, which agrees it).
        return Err(match local {
            Err(own) => own.escalated_by(&refused),
            Ok(()) => refused,
        });
    }
    let status = reported_severity(&local, severity_on_error);
    let r = match reduce_status(comm, phase_id(name), status, digest) {
        Ok(r) => r,
        Err(e) => {
            comm.mark_broken(&format!("agreement '{name}': {e}"));
            return Err(Error::fatal(FatalKind::Transport, e));
        }
    };
    let fatal = agreed_fatal(&r);
    interpret(name, local, &r, "schedule descriptor").map_err(|e| {
        if fatal {
            Error::fatal(FatalKind::Agreed, e)
        } else {
            e
        }
    })
}

/// Run one phase: `preflight` (local), agreement, then `communicate` on
/// every rank only if all preflights succeeded.
pub fn run_phase<P, T>(
    comm: &Communicator,
    name: &str,
    preflight: impl FnOnce() -> Result<P>,
    communicate: impl FnOnce(P) -> Result<T>,
) -> Result<T> {
    let pre = preflight();
    let (local, payload) = match pre {
        Ok(p) => (Ok(()), Some(p)),
        Err(e) => (Err(e), None),
    };
    agree(comm, name, local, Severity::Recoverable)?;
    match payload {
        Some(p) => communicate(p),
        None => Err(Error::Communication(format!(
            "phase '{name}': missing preflight payload"
        ))),
    }
}

/// State of a [`Phases`] scope.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ScopeState {
    Running,
    /// An agreement failed (on every rank, common knowledge) with this
    /// agreed worst severity (the fatal/recoverable distinction is
    /// structured state, identical on every rank).
    Failed(Severity),
    /// A transport failure, a transport-fatal local result or an
    /// unagreeable fatal result: communicator state unknown (fatal, local;
    /// the communicator is marked broken).
    Broken,
    /// Opened while the named scope was already live on this communicator:
    /// an independent nested (or concurrent) scope, refused before any
    /// communication.
    Refused(String),
}

/// Registration, step counter and failure state of one scope; shared by
/// the thread-confined [`Phases`] and the in-transit [`ParkedScope`].
#[derive(Debug)]
struct ScopeCore {
    comm: Communicator,
    scope: String,
    count: u64,
    state: ScopeState,
    /// Registration on the communicator (`None` when refused or born
    /// broken).
    uid: Option<u64>,
    /// The communicator family's mode when the scope opened (D-126).
    mode: CollectiveChecks,
    /// Outcomes deferred since the last agreement (commit points only).
    deferred: Option<Deferred>,
}

impl Drop for ScopeCore {
    fn drop(&mut self) {
        // An owner settles before it drops its scope (`Phases::settle`
        // agrees what is still deferred), unless the operation already
        // failed or broke on this rank.
        debug_assert!(
            self.deferred.is_none()
                || self.state != ScopeState::Running
                || std::thread::panicking(),
            "collective scope '{}' dropped with deferred outcomes not settled",
            self.scope
        );
        if let Some(uid) = self.uid {
            self.comm.close_scope(uid);
        }
    }
}

impl ScopeCore {
    fn is_failed(&self) -> bool {
        self.state != ScopeState::Running
    }

    fn failure(&self) -> Option<Severity> {
        match &self.state {
            ScopeState::Running => None,
            ScopeState::Failed(s) => Some(*s),
            ScopeState::Broken => Some(Severity::Fatal),
            ScopeState::Refused(_) => Some(Severity::Recoverable),
        }
    }

    fn fatal_kind(&self) -> Option<FatalKind> {
        match &self.state {
            ScopeState::Broken => Some(FatalKind::Transport),
            ScopeState::Failed(Severity::Fatal) => Some(FatalKind::Agreed),
            _ => None,
        }
    }
}

/// The phase sequence of one collective operation (see module docs).
///
/// A scope is registered on its communicator while it lives: opening a
/// second, independent scope on the same communicator — from any thread —
/// yields a **refused** scope whose operations fail locally without
/// communicating, and raw communicator operations are refused unless issued
/// by the owner thread inside one of the scope's steps.
/// Code that runs inside a collective operation communicates
/// through the scope it was handed.
///
/// `!Send + !Sync`: a live scope stays on the thread that registered it;
/// the only transfer is the explicit, checked [`Phases::park`] /
/// [`ParkedScope::resume`].
///
/// ```compile_fail,E0277
/// fn send<T: Send>() {}
/// send::<akantu_comm::phase::Phases>();
/// ```
///
/// ```compile_fail,E0277
/// fn sync<T: Sync>() {}
/// sync::<akantu_comm::phase::Phases>();
/// ```
///
/// ```
/// fn send<T: Send>() {}
/// send::<akantu_comm::phase::ParkedScope>();
/// ```
#[derive(Debug)]
pub struct Phases {
    core: ScopeCore,
    /// Thread confinement: `*const ` is neither `Send` nor
    /// `Sync`.
    _thread: PhantomData<*const ()>,
}

/// A live scope in transit between threads, produced by
/// [`Phases::park`].
///
/// It stays registered on its communicator, so while it is parked every
/// other scope and every raw operation on that communicator is refused on
/// every thread; no thread owns it until [`ParkedScope::resume`] makes the
/// resuming thread its owner. It carries the step counter and failure
/// state, so the scope keeps one effective owner and one failure state
/// across the transfer. Dropping it closes the scope.
#[derive(Debug)]
pub struct ParkedScope {
    core: ScopeCore,
}

impl ParkedScope {
    /// Resume the scope on the **current** thread, which becomes its owner
    /// (re-registered on the communicator; admission and raw-operation
    /// checks follow it). A transfer does not grant MPI thread permission:
    /// the lifecycle's thread policy still applies to every MPI call.
    #[must_use]
    pub fn resume(self) -> Phases {
        let mut core = self.core;
        if let Some(uid) = core.uid
            && core.comm.resume_scope(uid).is_err()
        {
            // Unreachable through the safe API (the parked scope uniquely
            // owns its registration); fail closed without communicating.
            core.uid = None;
            core.state = ScopeState::Broken;
        }
        Phases {
            core,
            _thread: PhantomData,
        }
    }

    /// Scope name.
    #[must_use]
    pub fn scope(&self) -> &str {
        &self.core.scope
    }

    /// See [`Phases::is_failed`].
    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.core.is_failed()
    }

    /// See [`Phases::failure`].
    #[must_use]
    pub fn failure(&self) -> Option<Severity> {
        self.core.failure()
    }

    /// See [`Phases::fatal_kind`].
    #[must_use]
    pub fn fatal_kind(&self) -> Option<FatalKind> {
        self.core.fatal_kind()
    }
}

impl Phases {
    /// New scope named `scope` on `comm` (identical name on every rank),
    /// owned by the current thread. Refused (see type docs) if another
    /// scope is live on `comm`; born broken (never communicating) if `comm`
    /// observed a transport failure.
    #[must_use]
    pub fn new(comm: &Communicator, scope: impl Into<String>) -> Self {
        let scope = scope.into();
        let (state, uid) = match comm.open_scope(&scope) {
            Ok(uid) => (ScopeState::Running, Some(uid)),
            Err(OpenRefusal::Active(active)) => (ScopeState::Refused(active), None),
            Err(OpenRefusal::Broken) => (ScopeState::Broken, None),
        };
        Self {
            core: ScopeCore {
                comm: comm.clone(),
                scope,
                count: 0,
                state,
                uid,
                mode: comm.collective_checks(),
                deferred: None,
            },
            _thread: PhantomData,
        }
    }

    /// Park this scope for an explicit transfer to another thread:
    /// the returned [`ParkedScope`] is `Send`, stays registered
    /// (refusing other scopes and raw operations on every thread) and is
    /// owned by no thread until [`ParkedScope::resume`].
    #[must_use]
    pub fn park(self) -> ParkedScope {
        if let Some(uid) = self.core.uid {
            self.core.comm.park_scope(uid);
        }
        ParkedScope { core: self.core }
    }

    /// Scope name.
    #[must_use]
    pub fn scope(&self) -> &str {
        &self.core.scope
    }

    /// Rank and size of the communicator (local information).
    #[must_use]
    pub fn rank_size(&self) -> (akantu_core::Rank, usize) {
        (self.core.comm.rank(), self.core.comm.size())
    }

    /// Number of agreements performed so far.
    #[must_use]
    pub fn agreements(&self) -> u64 {
        self.core.count
    }

    /// `true` after a failed agreement, a transport failure, or for a
    /// refused scope: the operation failed (on every rank, except for a
    /// transport failure); no further collective of this scope may be
    /// started.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.core.is_failed()
    }

    /// `true` after a transport failure (communicator state unknown).
    #[must_use]
    pub fn is_broken(&self) -> bool {
        self.core.state == ScopeState::Broken
    }

    /// `true` for a scope refused at creation (independent nested scope).
    #[must_use]
    pub fn is_refused(&self) -> bool {
        matches!(self.core.state, ScopeState::Refused(_))
    }

    /// `true` after an agreed [`Severity::Fatal`] failure (identical on
    /// every rank) or a transport failure: the owner must poison itself.
    #[must_use]
    pub fn is_fatal(&self) -> bool {
        self.core.fatal_kind().is_some()
    }

    /// Severity of the failure, if the scope failed (transport failures
    /// are fatal; a refused scope is recoverable).
    #[must_use]
    pub fn failure(&self) -> Option<Severity> {
        self.core.failure()
    }

    /// The owner's abort signal: `Transport` for a broken scope
    /// (only the observing ranks know; the runtime aborts), `Agreed` after
    /// an agreed fatal failure (identical on every rank), `None` otherwise.
    #[must_use]
    pub fn fatal_kind(&self) -> Option<FatalKind> {
        self.core.fatal_kind()
    }

    /// Owner exit: an error of a scope that is
    /// broken or failed fatally becomes `Error::Fatal` of
    /// [`Phases::fatal_kind`] (an already fatal error keeps the more urgent
    /// kind); recoverable failures and refusals are returned unchanged.
    /// Every owner that drops its scope settles its result first, so the
    /// classification is never lost with the scope.
    ///
    /// Under commit points (D-126) the outcomes still deferred are agreed
    /// here first, together with `r`'s own outcome: a scope's settle is the
    /// close of its operation.
    pub fn settle<R>(&mut self, r: Result<R>) -> Result<R> {
        let r = if self.core.deferred.is_some() && self.core.state == ScopeState::Running {
            match r {
                Ok(v) => self
                    .agree("settle", Ok(()), Severity::Recoverable)
                    .map(|()| v),
                Err(e) => Err(self
                    .agree("settle", Err(e), Severity::Recoverable)
                    .err()
                    .unwrap_or_else(|| Error::Communication("settle: lost error".into()))),
            }
        } else {
            r
        };
        r.map_err(|e| match self.fatal_kind() {
            Some(kind) => Error::fatal(kind, e),
            None => e,
        })
    }

    /// The classification of [`Phases::settle`] alone, without agreeing
    /// what is deferred: for an owner that classifies a step's result and
    /// keeps the operation running (it settles when it closes).
    pub fn classify<R>(&self, r: Result<R>) -> Result<R> {
        r.map_err(|e| match self.fatal_kind() {
            Some(kind) => Error::fatal(kind, e),
            None => e,
        })
    }

    /// The collective-check mode of this scope (its communicator family's
    /// when it opened).
    #[must_use]
    pub fn collective_checks(&self) -> CollectiveChecks {
        self.core.mode
    }

    /// `true` while outcomes of this scope wait for the next agreement.
    #[must_use]
    pub fn has_deferred(&self) -> bool {
        self.core.deferred.is_some()
    }

    /// Whether this scope defers local outcomes now: commit points, a
    /// communicating world, outside native library regions.
    fn defers(&self) -> bool {
        self.core.mode == CollectiveChecks::CommitPoints
            && !one_rank_without_mpi(&self.core.comm)
            && !in_native_region()
    }

    /// A **purely local** phase's outcome (D-126). Under
    /// [`CollectiveChecks::EveryPhase`] (and inside native regions) the
    /// same as [`Phases::agree`]; under commit points it is not
    /// communicated: it is folded into the scope's next agreement — before
    /// its next communication or commit point, or when it settles — and the
    /// rank's own error is returned at once (the other ranks learn of it
    /// there). Use only where nothing between this phase and the next
    /// agreement needs every rank's success: no point-to-point exchange
    /// outside the scope's own steps, no effect another rank relies on.
    pub fn defer(&mut self, name: &str, local: Result<()>, severity: Severity) -> Result<()> {
        self.defer_digest(name, local, 0, severity)
    }

    /// [`Phases::defer`] with a schedule `digest` (see
    /// [`Phases::agree_digest`]): compared at the agreement that receives
    /// the deferred outcome.
    pub fn defer_digest(
        &mut self,
        name: &str,
        local: Result<()>,
        digest: u64,
        severity: Severity,
    ) -> Result<()> {
        if !self.defers() {
            return self.agree_digest(name, local, digest, severity);
        }
        if let Err(e) = self.check(name) {
            if let Err(own) = &local {
                self.unagreed_fatal(own);
            }
            return Err(local.err().unwrap_or(e));
        }
        if let Err(own) = &local
            && is_transport(own)
        {
            self.unagreed_fatal(own);
            return local;
        }
        let status = reported_severity(&local, severity);
        let q = self.qualified(name);
        self.core.deferred = Some(Deferred::fold(self.core.deferred, &q, status, digest));
        DEFERRED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        local
    }

    /// [`Phases::defer`] on a local phase that produced a value.
    pub fn deferred<T>(&mut self, name: &str, local: Result<T>, severity: Severity) -> Result<T> {
        match local {
            Ok(v) => self.defer(name, Ok(()), severity).map(|()| v),
            Err(e) => Err(self
                .defer(name, Err(e), severity)
                .err()
                .unwrap_or_else(|| Error::Communication(format!("phase '{name}': lost error")))),
        }
    }

    /// A **commit point** (D-126): an immediate agreement in every mode
    /// (with the outcomes deferred so far), for a decision the ranks take
    /// together — a mesh transaction's commit, a solver's verdict — where a
    /// peer's failure must be known before anything irreversible happens.
    /// Same as [`Phases::agree`]; the name marks the call site.
    pub fn commit(&mut self, name: &str, local: Result<()>, severity: Severity) -> Result<()> {
        self.agree(name, local, severity)
    }

    /// `true` when a communication step may run without an agreement
    /// before it (D-126): commit points, a communicating world outside
    /// native regions, and nothing deferred since the last agreement — no
    /// rank can have left the operation, so a rank that failed locally just
    /// now is still here and reports its failure **in the exchange itself**
    /// (a status in each message; see `Synchronizer`).
    #[must_use]
    pub fn status_in_messages(&self) -> bool {
        self.defers() && self.core.deferred.is_none() && self.core.state == ScopeState::Running
    }

    /// A communication step without an agreement before it, allowed only
    /// while [`Phases::status_in_messages`] holds (otherwise the deferred
    /// outcomes are agreed first, as for [`Phases::collective`]). Every rank
    /// must run it, whatever its local outcome; `f`'s errors are transport
    /// failures (the scope breaks).
    pub fn exchange<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&Communicator) -> Result<R>,
    ) -> Result<R> {
        self.collective(name, f)
    }

    /// [`Phases::commit`] on an outcome that produced a value.
    pub fn commit_agreed<T>(
        &mut self,
        name: &str,
        local: Result<T>,
        severity: Severity,
    ) -> Result<T> {
        self.agreed(name, local, severity)
    }

    /// Agree what is deferred, if anything (before a communication step).
    fn flush(&mut self, name: &str) -> Result<()> {
        if self.core.deferred.is_some() && self.core.state == ScopeState::Running {
            self.agree(&format!("{name}:deferred"), Ok(()), Severity::Recoverable)
        } else {
            Ok(())
        }
    }

    /// Mark the scope broken (a communication primitive failed); the
    /// communicator is marked broken as well (no further scope or raw
    /// operation communicates on it).
    pub fn mark_broken(&mut self) {
        self.break_scope("marked broken by its owner");
    }

    fn break_scope(&mut self, reason: &str) {
        self.core.state = ScopeState::Broken;
        self.core
            .comm
            .mark_broken(&format!("scope '{}': {reason}", self.core.scope));
    }

    /// `Ok(())` while the scope may start phase `name`; otherwise the error
    /// that phase returns (nothing is communicated). A running scope whose
    /// communicator was broken meanwhile (a transport-fatal result reported
    /// to a refused nested scope) becomes broken here.
    pub fn check(&mut self, name: &str) -> Result<()> {
        if self.core.state == ScopeState::Running && self.core.comm.broken().is_some() {
            self.core.state = ScopeState::Broken;
        }
        match &self.core.state {
            ScopeState::Running => Ok(()),
            ScopeState::Failed(_) => Err(Error::Communication(format!(
                "{}: phase '{name}' not started, the operation already failed on all ranks",
                self.core.scope
            ))),
            ScopeState::Broken => Err(Error::Communication(format!(
                "{}: phase '{name}' not started, the communicator is in an unknown state",
                self.core.scope
            ))),
            ScopeState::Refused(active) => Err(Error::Communication(format!(
                "{}: phase '{name}' refused: collective scope '{active}' is already active on \
                 this communicator; communicate through it instead of an independent scope",
                self.core.scope
            ))),
        }
    }

    /// A local fatal outcome reaching a scope that cannot agree it:
    /// a transport-fatal error always breaks the scope and the communicator
    /// (never agreed); an agreed-kind fatal error after the operation
    /// already failed recoverably can no longer reach the peers and takes
    /// the abort path as well. A refused scope passes agreed-kind errors on
    /// to the enclosing scope, which agrees them.
    fn unagreed_fatal(&mut self, own: &Error) {
        match (own.fatal_kind(), &self.core.state) {
            (Some(FatalKind::Transport), _) => {
                self.break_scope(&format!("transport-fatal local result: {own}"));
            }
            (Some(FatalKind::Agreed), ScopeState::Failed(Severity::Recoverable)) => {
                self.break_scope(&format!(
                    "fatal local result after a recoverable failure cannot be agreed: {own}"
                ));
            }
            _ => {}
        }
    }

    fn qualified(&mut self, name: &str) -> String {
        let q = format!("{}#{}:{name}", self.core.scope, self.core.count);
        self.core.count += 1;
        q
    }

    /// Agreement on `local` (see [`agree`]); records a failure.
    pub fn agree(&mut self, name: &str, local: Result<()>, severity: Severity) -> Result<()> {
        self.agree_digest(name, local, 0, severity)
    }

    /// [`Phases::agree`] on a local step that produced a value: returns the
    /// value iff every rank succeeded (the rank's own error otherwise).
    pub fn agreed<T>(&mut self, name: &str, local: Result<T>, severity: Severity) -> Result<T> {
        match local {
            Ok(v) => self.agree(name, Ok(()), severity).map(|()| v),
            Err(e) => Err(self
                .agree(name, Err(e), severity)
                .err()
                .unwrap_or_else(|| Error::Communication(format!("phase '{name}': lost error")))),
        }
    }

    /// Agreement on `local` and on a schedule `digest` (see
    /// [`agree_digest`]); records a failure and its agreed severity. On a
    /// failed, broken or refused scope nothing is communicated and the
    /// rank's own error (if any) is returned.
    ///
    /// The reported severity follows the central rule (module docs):
    /// a local `Error::Fatal { kind: Agreed }` is reported as
    /// [`Severity::Fatal`] whatever `severity` says, and a local
    /// `Error::Fatal { kind: Transport }` breaks the scope and the
    /// communicator without communicating.
    pub fn agree_digest(
        &mut self,
        name: &str,
        local: Result<()>,
        digest: u64,
        severity: Severity,
    ) -> Result<()> {
        if let Err(e) = self.check(name) {
            if let Err(own) = &local {
                self.unagreed_fatal(own);
            }
            // Keep the rank's own error when it has one.
            return Err(local.err().unwrap_or(e));
        }
        if let Err(own) = &local
            && is_transport(own)
        {
            self.unagreed_fatal(own);
            return local;
        }
        let status = reported_severity(&local, severity);
        if one_rank_without_mpi(&self.core.comm) {
            // One rank without MPI: the agreement is this rank's own outcome
            // (what the one-rank all-reduce returns), without the all-reduce,
            // the qualified name or the step guard.
            self.core.count += 1;
            count_agreement(None);
            if local.is_err() {
                self.core.state = ScopeState::Failed(if status >= Severity::Fatal {
                    Severity::Fatal
                } else {
                    Severity::Recoverable
                });
            }
            return local;
        }
        let q = self.qualified(name);
        // D-126: the outcomes deferred since the last agreement are agreed
        // with this one
        let (status, id, digest) = match self.core.deferred.take() {
            None => (status, phase_id(&q), digest),
            Some(d) => {
                let f = Deferred::fold(Some(d), &q, status, digest);
                (f.worst, f.id, f.digest)
            }
        };
        let reduced = {
            let _step = self.core.comm.scope_step(self.core.uid);
            reduce_status(&self.core.comm, id, status, digest)
        };
        let r = match reduced {
            Ok(r) => r,
            Err(e) => {
                self.break_scope(&format!("agreement '{q}': {e}"));
                // The transport failure decides the outcome.
                return Err(e);
            }
        };
        let out = interpret(&q, local, &r, "schedule descriptor");
        if out.is_err() {
            // Agreed severity: the worst reported status, identical on
            // every rank. Ranks in different phases while all reporting
            // success is a protocol divergence: fatal.
            self.core.state = ScopeState::Failed(if agreed_fatal(&r) {
                Severity::Fatal
            } else {
                Severity::Recoverable
            });
        }
        out
    }

    /// One phase: agree on the locally `prepared` result (validation,
    /// prepacked buffers, sizes), then — only if every rank succeeded —
    /// `communicate`, then agree on its post-processing result.
    ///
    /// `communicate` returns `Result<Result<R>>`: the **outer** `Err` is a
    /// transport failure (the scope becomes broken, no post agreement is
    /// attempted); the **inner** result is local post-processing (unpack,
    /// validation of received data) after all communication completed,
    /// agreed on every rank. `communicate` must complete (or cancel) every
    /// request it posted before returning.
    pub fn run<P, R>(
        &mut self,
        name: &str,
        prepared: Result<P>,
        communicate: impl FnOnce(&Communicator, P) -> Result<Result<R>>,
    ) -> Result<R> {
        let payload = self.agreed(&format!("{name}:prepare"), prepared, Severity::Recoverable)?;
        let outcome = {
            let _step = self.core.comm.scope_step(self.core.uid);
            communicate(&self.core.comm, payload)
        };
        let inner = match outcome {
            Ok(inner) => inner,
            Err(e) => {
                self.break_scope(&format!("phase '{name}': {e}"));
                return Err(e);
            }
        };
        // the post-processing is local (every request completed): under
        // commit points its outcome waits for the next agreement (D-126)
        self.deferred(&format!("{name}:post"), inner, Severity::Recoverable)
    }

    /// Agreement on the locally `prepared` result, then — only if every
    /// rank succeeded — the matched collective `f` (no post agreement:
    /// `f`'s result is the collective's own; any error is a transport
    /// failure). The usual form of a checked reduction (a norm, a count).
    ///
    /// Under commit points (D-126) the agreement is not a collective of its
    /// own: the outcomes deferred so far and `prepared`'s are carried in the
    /// header of `f`'s first checked collective, which every rank combines
    /// before any payload moves. A rank whose `prepared` failed joins with a
    /// header-only collective; a failure reported by any rank fails the
    /// scope on every rank with the agreed severity, as the separate
    /// agreement does. `f` must issue a checked collective on the scope's
    /// communicator first, on every rank.
    pub fn run_collective<P, R>(
        &mut self,
        name: &str,
        prepared: Result<P>,
        f: impl FnOnce(&Communicator, P) -> Result<R>,
    ) -> Result<R> {
        if !self.defers() {
            let payload =
                self.agreed(&format!("{name}:prepare"), prepared, Severity::Recoverable)?;
            return self.collective(name, |c| f(c, payload));
        }
        let (payload, mut local) = match prepared {
            Ok(p) => (Some(p), Ok(())),
            Err(e) => (None, Err(e)),
        };
        if let Err(e) = self.check(name) {
            if let Err(own) = &local {
                self.unagreed_fatal(own);
            }
            return Err(local.err().unwrap_or(e));
        }
        if let Err(own) = &local
            && is_transport(own)
        {
            self.unagreed_fatal(own);
            return Err(local
                .err()
                .unwrap_or_else(|| Error::Communication("lost error".into())));
        }
        let status = reported_severity(&local, Severity::Recoverable);
        let q = self.qualified(&format!("{name}:prepare"));
        let folded = Deferred::fold(self.core.deferred.take(), &q, status, 0);
        let comm = self.core.comm.clone();
        arm_status(
            &comm,
            HeaderStatus {
                severity: folded.worst as u8,
                id: folded.id,
                digest: folded.digest,
            },
        );
        let t = std::time::Instant::now();
        let outcome = {
            let _step = comm.scope_step(self.core.uid);
            match payload {
                Some(p) => f(&comm, p).map(Some),
                // header only: the armed status reaches the peers' collective
                None => comm.barrier().map(|()| None),
            }
        };
        count_agreement(Some(t));
        let unconsumed = disarm_status();
        if let Some(v) = take_verdict() {
            let r = Reduced {
                worst: u64::from(v.worst),
                ids_match: v.ids_match,
                digests_match: v.digests_match,
            };
            self.core.state = ScopeState::Failed(if agreed_fatal(&r) {
                Severity::Fatal
            } else {
                Severity::Recoverable
            });
            return Err(interpret(
                &q,
                std::mem::replace(&mut local, Ok(())),
                &r,
                "schedule descriptor",
            )
            .err()
            .unwrap_or_else(|| Error::Communication(format!("phase '{q}': lost verdict"))));
        }
        if unconsumed {
            self.break_scope(&format!(
                "collective '{name}': no checked collective carried the folded agreement"
            ));
            return Err(Error::fatal(
                FatalKind::Transport,
                Error::Communication(format!(
                    "collective '{name}' issued no checked collective on its communicator"
                )),
            ));
        }
        match outcome {
            Ok(Some(v)) => Ok(v),
            Ok(None) => Err(local
                .err()
                .unwrap_or_else(|| Error::Communication(format!("phase '{q}': lost local error")))),
            Err(e) => {
                self.break_scope(&format!("collective '{name}': {e}"));
                Err(e)
            }
        }
    }

    /// A matched collective (e.g. an all-reduce of counts) whose
    /// participation every rank already agreed on **immediately before**
    /// (no fallible local step since the last agreement on any rank). Any
    /// error is a transport failure (scope broken).
    ///
    /// Under commit points the outcomes deferred since the last agreement
    /// are agreed first (D-126).
    pub fn collective<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&Communicator) -> Result<R>,
    ) -> Result<R> {
        self.flush(name)?;
        self.check(name)?;
        let outcome = {
            let _step = self.core.comm.scope_step(self.core.uid);
            f(&self.core.comm)
        };
        match outcome {
            Ok(v) => Ok(v),
            Err(e) => {
                self.break_scope(&format!("collective '{name}': {e}"));
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communicator::{Tag, run_local};
    use std::time::{Duration, Instant};

    /// Two-phase exchange: (1) agree on message sizes, (2) send/receive.
    /// Rank `bad` fails its preflight **before packing**; without the
    /// agreement its peers would wait for a message that is never sent.
    fn exchange(c: &Communicator, bad: Option<i32>, fail_between: bool) -> Result<Vec<f64>> {
        let peer = (c.rank() + 1) % 2;
        let n = run_phase(
            c,
            "sizes",
            || {
                if Some(c.rank()) == bad {
                    Err(Error::InvalidArgument("local validation failed".into()))
                } else {
                    Ok(3usize)
                }
            },
            Ok,
        )?;
        let local_post = if fail_between && c.rank() == 1 {
            Err(Error::Numerical("NaN in packed data".into()))
        } else {
            Ok(())
        };
        agree(c, "pack", local_post, Severity::Recoverable)?;
        c.send(&vec![f64::from(c.rank()); n], peer, Tag(1))?;
        let mut r = vec![0.0; n];
        c.receive(&mut r, peer, Tag(1))?;
        Ok(r)
    }

    #[test]
    fn injected_failures_complete_on_all_ranks() {
        for (bad, between) in [
            (None, false),
            (Some(0), false),
            (Some(1), false),
            (None, true),
        ] {
            let t0 = Instant::now();
            let out = run_local(2, Duration::from_secs(5), |c| exchange(&c, bad, between));
            assert!(
                t0.elapsed() < Duration::from_secs(2),
                "no rank waited for a timeout"
            );
            let ok = bad.is_none() && !between;
            assert_eq!(
                out.iter().filter(|r| r.is_ok()).count(),
                if ok { 2 } else { 0 }
            );
            if let Some(b) = bad {
                let own = out[usize::try_from(b).unwrap()].as_ref().unwrap_err();
                assert!(
                    matches!(own, Error::InvalidArgument(_)),
                    "failing rank keeps its error"
                );
            }
        }
    }

    /// A rank failing locally before its consumer-internal phase (rank 0)
    /// reaches the owner's post agreement while rank 1 is in the
    /// consumer's agreement: both fail at that same step, rank 1's scope is
    /// failed, so it skips the owner's post agreement — no rank waits.
    #[test]
    fn failure_in_different_code_paths_is_bounded() {
        let t0 = Instant::now();
        let out = run_local(2, Duration::from_secs(10), |c| {
            let mut ph = Phases::new(&c, "op");
            let consumer = if c.rank() == 0 {
                Err(Error::Mesh("re-entrant mutation refused locally".into()))
            } else {
                ph.run("consumer:exchange", Ok(()), |comm, ()| {
                    let mut v = [1u64];
                    comm.all_reduce(&mut v, ReduceOp::Sum)?;
                    Ok(Ok(v[0]))
                })
                .map(|_| ())
            };
            let post = if ph.is_failed() {
                consumer
            } else {
                ph.agree("owner:post", consumer, Severity::Fatal)
            };
            (
                post.is_err(),
                ph.is_failed(),
                ph.agree("later", Ok(()), Severity::Fatal).is_err(),
            )
        });
        assert!(t0.elapsed() < Duration::from_secs(2), "no timeout involved");
        for (failed, scope_failed, later_refused) in out {
            assert!(failed && scope_failed && later_refused);
        }
    }

    #[test]
    fn digest_and_post_failures_are_agreed() {
        let out = run_local(2, Duration::from_secs(10), |c| {
            c.set_collective_checks(CollectiveChecks::EveryPhase);
            let mut ph = Phases::new(&c, "op");
            let d = ph.agree_digest(
                "schedule",
                Ok(()),
                u64::from(c.rank() == 1),
                Severity::Recoverable,
            );
            assert_eq!(ph.failure(), Some(Severity::Recoverable));
            drop(ph); // one owner scope per communicator and thread
            let mut ph2 = Phases::new(&c, "op2");
            let post = ph2.run("unpack", Ok(()), |comm, ()| {
                comm.barrier()?;
                Ok(if comm.rank() == 0 {
                    Err(Error::Numerical("bad received value".into()))
                } else {
                    Ok(())
                })
            });
            (d.is_err(), post.is_err(), ph2.is_failed(), ph2.is_broken())
        });
        for o in out {
            assert_eq!(o, (true, true, true, false));
        }
    }

    /// The agreed severity is structured state, identical on every
    /// rank (a fatal failure on one rank makes every rank's scope fatal).
    #[test]
    fn agreed_severity_is_common_knowledge() {
        let out = run_local(2, Duration::from_secs(10), |c| {
            let mut fatal = Phases::new(&c, "fatal");
            let local = if c.rank() == 1 {
                Err(Error::Numerical("irreversible step failed".into()))
            } else {
                Ok(())
            };
            let r = fatal.agree("commit", local, Severity::Fatal);
            let a = (r.is_err(), fatal.is_fatal(), fatal.failure());
            drop(fatal);
            let mut rec = Phases::new(&c, "recoverable");
            let local = if c.rank() == 0 {
                Err(Error::invalid("bad input"))
            } else {
                Ok(())
            };
            let r = rec.agree("preflight", local, Severity::Recoverable);
            (a, (r.is_err(), rec.is_fatal(), rec.failure()))
        });
        for (a, b) in out {
            assert_eq!(a, (true, true, Some(Severity::Fatal)));
            assert_eq!(b, (true, false, Some(Severity::Recoverable)));
        }
    }

    /// An independent scope opened inside an active one, and raw
    /// communication outside the active scope's steps, are refused locally
    /// (nothing is sent); the enclosing scope's next agreement reports the
    /// failure on every rank, also when only one rank took that path.
    #[test]
    fn nested_independent_scopes_and_raw_operations_are_refused() {
        let t0 = Instant::now();
        let out = run_local(2, Duration::from_secs(20), |c| {
            let mut owner = Phases::new(&c, "owner");
            assert_eq!(c.active_scope().as_deref(), Some("owner"));
            // A callback on rank 0 uses its own scope / a raw collective;
            // rank 1's callback communicates through the owner's scope.
            let callback = if c.rank() == 0 {
                let mut nested = Phases::new(&c, "callback-sync");
                assert!(nested.is_refused() && nested.is_failed());
                let e1 = nested
                    .agree("x", Ok(()), Severity::Recoverable)
                    .unwrap_err();
                let mut v = [1u64];
                let e2 = c.all_reduce(&mut v, ReduceOp::Sum).unwrap_err();
                let e3 = agree(&c, "free", Ok(()), Severity::Recoverable).unwrap_err();
                assert!(e1.to_string().contains("already active"), "{e1}");
                assert!(e2.to_string().contains("raw all_reduce refused"), "{e2}");
                assert!(e3.to_string().contains("refused"), "{e3}");
                Err(e2)
            } else {
                owner.run("callback:exchange", Ok(()), |comm, ()| {
                    let mut v = [1u64];
                    comm.all_reduce(&mut v, ReduceOp::Sum)?; // inside a step: allowed
                    Ok(Ok(()))
                })
            };
            let r = owner.agree("callback", callback, Severity::Recoverable);
            let later = owner.agree("later", Ok(()), Severity::Recoverable).is_err();
            drop(owner);
            // Once the owner is gone, scopes and raw operations work again.
            let mut v = [1u64];
            c.all_reduce(&mut v, ReduceOp::Sum).unwrap();
            (r.is_err(), later, v[0], c.active_scope())
        });
        assert!(t0.elapsed() < Duration::from_secs(2), "no timeout involved");
        for o in out {
            assert_eq!(o, (true, true, 2, None));
        }
    }

    // ------------------------------------------------- scope ownership

    /// The scope-loan choreography without the (uncompilable)
    /// scope loan: the worker only has the communicator. While `outer` is
    /// live on the owner thread, the worker's independent scope and raw
    /// operations are refused on **every** thread (communicator-wide
    /// admission), so rank 1 never pairs a child agreement with rank 0's
    /// outer agreement; the owner's agreement reports both failures on
    /// both ranks, with no timeout.
    #[test]
    fn worker_without_the_scope_is_refused_and_the_owner_agrees() {
        let t0 = Instant::now();
        let out = run_local(2, Duration::from_millis(250), |c| {
            let mut outer = Phases::new(&c, "outer");
            let local = std::thread::scope(|s| {
                s.spawn(|| {
                    assert_eq!(c.active_scope().as_deref(), Some("outer"));
                    assert!(!c.owns_active_scope());
                    assert!(
                        c.barrier()
                            .unwrap_err()
                            .to_string()
                            .contains("owned by another thread")
                    );
                    if c.rank() == 0 {
                        Err(Error::invalid("local callback failure"))
                    } else {
                        let mut nested = Phases::new(&c, "independent-child");
                        assert!(nested.is_refused(), "the missing enforcement");
                        nested.agree("child", Ok(()), Severity::Recoverable)
                    }
                })
                .join()
                .unwrap()
            });
            let r = outer.agree("callback:done", local, Severity::Recoverable);
            let failed = outer.is_failed();
            let later = outer.agree("later", Ok(()), Severity::Recoverable);
            (r.map_err(|e| e.to_string()), failed, later.is_err())
        });
        assert!(t0.elapsed() < Duration::from_millis(200), "no timeout");
        assert!(
            out[0]
                .0
                .as_ref()
                .unwrap_err()
                .contains("local callback failure")
        );
        assert!(out[1].0.as_ref().unwrap_err().contains("already active"));
        for o in &out {
            assert!(o.1 && o.2);
        }
    }

    /// The scope-loan choreography through the **explicit transfer**:
    /// the owner parks `outer`, the worker resumes it (becoming its owner
    /// on the communicator), rank 1's worker opens an independent child
    /// (refused: the transferred scope is live), and the outer agreement on
    /// the worker fails on both ranks. Parked back and resumed on the
    /// owner thread, the scope keeps its failure state: the later
    /// agreement returns at once on both ranks, nothing times out.
    #[test]
    fn explicit_transfer_keeps_one_owner_and_its_failure_state() {
        let t0 = Instant::now();
        let out = run_local(2, Duration::from_millis(250), |c| {
            let parked = Phases::new(&c, "outer").park();
            // In transit: still live, owned by nobody; refused everywhere.
            assert!(!c.owns_active_scope());
            assert!(c.barrier().unwrap_err().to_string().contains("parked"));
            assert!(Phases::new(&c, "x").is_refused());
            let (parked, r) = std::thread::scope(|s| {
                s.spawn(|| {
                    let mut outer = parked.resume();
                    assert!(c.owns_active_scope(), "admission follows the transfer");
                    let local = if c.rank() == 0 {
                        Err(Error::invalid("local callback failure"))
                    } else {
                        let mut nested = Phases::new(&c, "independent-child");
                        assert!(nested.is_refused());
                        nested.agree("child", Ok(()), Severity::Recoverable)
                    };
                    let r = outer.agree("callback:done", local, Severity::Recoverable);
                    (outer.park(), r.is_err())
                })
                .join()
                .unwrap()
            });
            assert!(parked.is_failed() && parked.failure() == Some(Severity::Recoverable));
            let mut outer = parked.resume();
            assert!(c.owns_active_scope());
            let later = outer.agree("later", Ok(()), Severity::Recoverable).is_err();
            drop(outer);
            assert_eq!(c.active_scope(), None);
            (r, later)
        });
        assert!(t0.elapsed() < Duration::from_millis(200), "no timeout");
        assert_eq!(out, vec![(true, true), (true, true)]);
    }

    /// A successful transfer continues the same step sequence: agreements
    /// before, on and after the worker pair up on both ranks.
    #[test]
    fn transferred_scope_continues_its_step_sequence() {
        let out = run_local(2, Duration::from_secs(5), |c| {
            let mut ph = Phases::new(&c, "op");
            ph.agree("before", Ok(()), Severity::Recoverable).unwrap();
            let parked = ph.park();
            let parked = std::thread::scope(|s| {
                s.spawn(|| {
                    let mut ph = parked.resume();
                    let sum = ph
                        .run("exchange", Ok(()), |comm, ()| {
                            let mut v = [u64::try_from(comm.rank()).unwrap() + 1];
                            comm.all_reduce(&mut v, ReduceOp::Sum)?;
                            Ok(Ok(v[0]))
                        })
                        .unwrap();
                    (ph.park(), sum)
                })
                .join()
                .unwrap()
            });
            let (parked, sum) = parked;
            let mut ph = parked.resume();
            ph.agree("after", Ok(()), Severity::Recoverable).unwrap();
            (sum, ph.agreements())
        });
        assert_eq!(out, vec![(3, 4), (3, 4)]);
    }

    // ------------------------------------------------- double completion

    /// A local `Error::Fatal { kind: Agreed }` passed with
    /// `Severity::Recoverable` (a generic callback boundary) is reported
    /// fatal: every rank's scope is fatal and settles to `Agreed`; an
    /// ordinary error at the same boundary stays recoverable.
    #[test]
    fn typed_fatal_local_results_are_agreed_fatal() {
        for typed in [true, false] {
            let out = run_local(2, Duration::from_secs(5), |c| {
                let mut ph = Phases::new(&c, "op");
                let local = if c.rank() == 0 {
                    let e = Error::invalid("callback failure");
                    Err(if typed {
                        Error::fatal(FatalKind::Agreed, e)
                    } else {
                        e
                    })
                } else {
                    Ok(())
                };
                let r = ph.agree("callback", local, Severity::Recoverable);
                (
                    ph.fatal_kind(),
                    ph.settle(r).unwrap_err().fatal_kind(),
                    c.broken().is_none(),
                )
            });
            let k = typed.then_some(FatalKind::Agreed);
            assert_eq!(out, vec![(k, k, true), (k, k, true)], "typed: {typed}");
        }
    }

    /// A local transport-fatal result is never agreed: the reporting rank
    /// breaks its scope and the communicator without communicating (the
    /// abort path; its peer, still waiting, observes the transport failure
    /// itself), every later scope on that communicator is born broken and
    /// raw operations are refused with the transport kind.
    #[test]
    fn transport_fatal_local_result_takes_the_abort_path() {
        let out = run_local(2, Duration::from_millis(250), |c| {
            let t0 = Instant::now();
            let mut ph = Phases::new(&c, "op");
            let local = if c.rank() == 0 {
                Err(Error::fatal(
                    FatalKind::Transport,
                    Error::Communication("peer lost".into()),
                ))
            } else {
                Ok(())
            };
            let r = ph.agree("callback", local, Severity::Recoverable);
            let settled = ph.settle(r).unwrap_err();
            let fast = t0.elapsed() < Duration::from_millis(100);
            drop(ph);
            let reborn = Phases::new(&c, "again");
            let raw = c.barrier().unwrap_err();
            (
                settled.fatal_kind(),
                fast,
                reborn.is_broken(),
                raw.fatal_kind(),
                settled.to_string().contains("timed out"),
            )
        });
        let t = Some(FatalKind::Transport);
        assert_eq!(out[0], (t, true, true, t, false), "never communicated");
        assert_eq!(out[1], (t, false, true, t, true), "peer timed out");
    }

    /// A fatal outcome reaching a scope that already failed recoverably
    /// cannot be agreed any more: it takes the abort path locally
    /// (transport kind, communicator broken) instead of returning a fatal
    /// error the peers never learn about as an "agreed" one.
    #[test]
    fn unagreeable_fatal_outcome_takes_the_abort_path() {
        let out = run_local(2, Duration::from_secs(5), |c| {
            let mut ph = Phases::new(&c, "op");
            let local = if c.rank() == 1 {
                Err(Error::invalid("bad input"))
            } else {
                Ok(())
            };
            let _ = ph.agree("preflight", local, Severity::Recoverable);
            let late = if c.rank() == 0 {
                Err(Error::fatal(FatalKind::Agreed, Error::invalid("late")))
            } else {
                Ok(())
            };
            let r = ph.agree("late", late, Severity::Recoverable);
            (ph.settle(r).unwrap_err().fatal_kind(), c.broken().is_some())
        });
        assert_eq!(out[0], (Some(FatalKind::Transport), true));
        assert_eq!(out[1], (None, false));
    }

    /// Free-function agreements follow the same rule and classify their
    /// own outcome (no owner settles them).
    #[test]
    fn free_agreements_classify_fatal_outcomes() {
        let out = run_local(2, Duration::from_secs(5), |c| {
            let local = if c.rank() == 1 {
                Err(Error::fatal(FatalKind::Agreed, Error::invalid("x")))
            } else {
                Ok(())
            };
            let a = agree(&c, "typed", local, Severity::Recoverable).unwrap_err();
            let local = if c.rank() == 1 {
                Err(Error::invalid("x"))
            } else {
                Ok(())
            };
            let b = agree(&c, "plain", local, Severity::Recoverable).unwrap_err();
            (a.fatal_kind(), b.fatal_kind())
        });
        for o in out {
            assert_eq!(o, (Some(FatalKind::Agreed), None));
        }
    }

    // ------------------------------------------------------------------

    /// The free agreement entry points on one handle: `agree`,
    /// `agree_digest` and `run_phase`'s preflight agreement.
    fn free_entry(c: &Communicator, which: &str, local: Result<()>) -> Result<()> {
        match which {
            "agree" => agree(c, "later", local, Severity::Recoverable),
            "agree_digest" => agree_digest(c, "later", local, 7, Severity::Recoverable),
            _ => run_phase(c, "later", || local, |()| Ok(())),
        }
    }

    /// Local outcomes: success, an ordinary failure, an agreed-kind fatal
    /// failure.
    fn free_locals() -> [(&'static str, Result<()>); 3] {
        [
            ("ok", Ok(())),
            ("ordinary", Err(Error::invalid("local validation"))),
            (
                "agreed",
                Err(Error::fatal(
                    FatalKind::Agreed,
                    Error::invalid("local validation"),
                )),
            ),
        ]
    }

    /// A free agreement on a communicator that already
    /// observed a transport failure returns `Error::Fatal { kind: Transport
    /// }` whatever the local outcome (transport over agreed over
    /// recoverable), keeps the local diagnostic, and communicates nothing.
    #[test]
    fn free_agreements_on_a_broken_communicator_stay_transport_fatal() {
        for which in ["agree", "agree_digest", "run_phase"] {
            for (what, local) in free_locals() {
                let c = Communicator::serial();
                let mut ph = Phases::new(&c, "outer");
                ph.mark_broken();
                drop(ph);
                let e = free_entry(&c, which, local).unwrap_err();
                assert_eq!(
                    e.fatal_kind(),
                    Some(FatalKind::Transport),
                    "{which}, {what}: {e}"
                );
                if what != "ok" {
                    assert!(e.to_string().contains("local validation"), "{e}");
                }
                assert_eq!(
                    c.barrier().unwrap_err().fatal_kind(),
                    Some(FatalKind::Transport)
                );
            }
        }
    }

    /// Control: a refusal on a **healthy** communicator (a scope
    /// is live) stays an ordinary local error; a local error keeps its own
    /// class (an agreed-kind fatal error is passed on to the enclosing
    /// scope, which agrees it), never escalated to transport.
    #[test]
    fn free_agreements_refused_by_a_live_scope_keep_the_local_class() {
        for which in ["agree", "agree_digest", "run_phase"] {
            for (what, local) in free_locals() {
                let c = Communicator::serial();
                let _owner = Phases::new(&c, "owner");
                let e = free_entry(&c, which, local).unwrap_err();
                let expected = (what == "agreed").then_some(FatalKind::Agreed);
                assert_eq!(e.fatal_kind(), expected, "{which}, {what}: {e}");
                assert!(c.broken().is_none());
            }
        }
    }

    #[test]
    fn phase_mismatch_is_detected() {
        let out = run_local(2, Duration::from_secs(5), |c| {
            let name = if c.rank() == 0 { "a" } else { "b" };
            agree(&c, name, Ok(()), Severity::Recoverable)
        });
        assert!(out.iter().all(Result::is_err));
        assert_ne!(phase_id("a"), phase_id("b"));
    }

    /// An agreed, ordinary error of a raw collective in a
    /// communication step fails the scope recoverably (communicator usable,
    /// later scopes run); a transport error still breaks it.
    #[test]
    fn agreed_error_of_a_raw_collective_is_an_inner_result() {
        let out = run_local(2, Duration::from_secs(10), |c| {
            c.set_collective_checks(CollectiveChecks::EveryPhase);
            let mut ph = Phases::new(&c, "pe01");
            let r: Result<()> = ph.run("native", Ok(()), |_, ()| {
                agreed_or_transport::<()>(Err(Error::Communication(
                    "refused under MPI_THREAD_SERIALIZED (agreed by the header)".into(),
                )))
            });
            let failed = r.is_err() && !ph.is_fatal();
            drop(ph);
            let usable = c.broken().is_none() && c.barrier().is_ok();
            let mut ph2 = Phases::new(&c, "pe01-transport");
            let t: Result<()> = ph2.run("native", Ok(()), |_, ()| {
                agreed_or_transport::<()>(Err(Error::fatal(
                    FatalKind::Transport,
                    Error::Communication("MPI_Allreduce failed".into()),
                )))
            });
            drop(ph2);
            (failed, usable, t.is_err() && c.broken().is_some())
        });
        for (rank, (failed, usable, broken)) in out.into_iter().enumerate() {
            assert!(
                failed,
                "rank {rank}: the agreed error did not fail the scope recoverably"
            );
            assert!(
                usable,
                "rank {rank}: the communicator is unusable after an agreed error"
            );
            assert!(
                broken,
                "rank {rank}: a transport error must break the communicator"
            );
        }
    }

    /// D-126 commit points: a post-processing failure of one rank stays
    /// local until the scope settles; then every rank fails recoverably,
    /// the failing rank with its own error.
    #[test]
    fn commit_points_post_failure_is_agreed_when_the_scope_settles() {
        let out = run_local(2, Duration::from_secs(10), |c| {
            c.set_collective_checks(CollectiveChecks::CommitPoints);
            let mut ph = Phases::new(&c, "op2");
            let post = ph.run("unpack", Ok(()), |comm, ()| {
                comm.barrier()?;
                Ok(if comm.rank() == 0 {
                    Err(Error::Numerical("bad received value".into()))
                } else {
                    Ok(())
                })
            });
            let deferred = ph.has_deferred() && !ph.is_failed();
            let settled = ph.settle(
                post.as_ref()
                    .map(|()| ())
                    .map_err(|e| Error::Numerical(e.to_string())),
            );
            let own = settled
                .as_ref()
                .err()
                .is_some_and(|e| e.to_string().contains("bad received value"));
            (
                post.is_err(),
                deferred,
                settled.is_err(),
                own,
                ph.failure(),
                ph.is_broken(),
            )
        });
        assert_eq!(
            out[0],
            (true, true, true, true, Some(Severity::Recoverable), false)
        );
        assert_eq!(out[1].0, false, "rank 1's own post-processing succeeded");
        assert_eq!((out[1].1, out[1].2, out[1].3), (true, true, false));
        assert_eq!((out[1].4, out[1].5), (Some(Severity::Recoverable), false));
    }

    /// D-126 commit points: an inner (header-agreed) error of a raw
    /// collective is deferred like any post-processing result and fails
    /// the scope recoverably on every rank when it settles; the
    /// communicator stays usable. A transport error still breaks it at once.
    #[test]
    fn commit_points_agreed_inner_error_settles_recoverably() {
        let out = run_local(2, Duration::from_secs(10), |c| {
            c.set_collective_checks(CollectiveChecks::CommitPoints);
            let mut ph = Phases::new(&c, "pe01");
            let r: Result<()> = ph.run("native", Ok(()), |_, ()| {
                agreed_or_transport::<()>(Err(Error::Communication(
                    "refused under MPI_THREAD_SERIALIZED (agreed by the header)".into(),
                )))
            });
            let r = ph.settle(r);
            let failed = r.is_err() && ph.is_failed() && !ph.is_fatal();
            drop(ph);
            let usable = c.broken().is_none() && c.barrier().is_ok();
            let mut ph2 = Phases::new(&c, "pe01-transport");
            let t: Result<()> = ph2.run("native", Ok(()), |_, ()| {
                agreed_or_transport::<()>(Err(Error::fatal(
                    FatalKind::Transport,
                    Error::Communication("MPI_Allreduce failed".into()),
                )))
            });
            drop(ph2);
            (failed, usable, t.is_err() && c.broken().is_some())
        });
        for o in out {
            assert_eq!(o, (true, true, true));
        }
    }
}
