// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/communicator.hh,
// communicator.cc, communicator_dummy_inline_impl.hh,
// communicator_mpi_inline_impl.hh, communicator_inline_impl.hh,
// communicator_event_handler.hh, communication_tag.hh,
// communication_request.hh.

//! The [`Communicator`] handle (decision record D-015).
//!
//! Backends:
//!
//! * **serial** (default, upstream `CommunicatorDummy`): collectives are
//!   identities; point-to-point messages to self go through a mailbox; a
//!   receive without a matching send is an error instead of a dead-lock;
//! * **local world** (always available): `n` ranks as threads of one
//!   process, used by tests and examples to exercise genuinely distributed
//!   code paths without MPI. Collectives are implemented by gathering to
//!   rank 0 and reducing **in rank order**, so results are deterministic;
//!   every blocking wait has a timeout so a protocol error surfaces as
//!   `Error::Communication` instead of a hang. Point-to-point sends are
//!   either buffered ([`SendMode::Eager`], the default) or
//!   [`SendMode::Rendezvous`]: a send completes only once the matching
//!   receive consumed it, like a large MPI message beyond the eager limit —
//!   protocols that would dead-lock without eager buffering (blocking
//!   send-all before receive-all) time out in that mode;
//! * **MPI** (feature `mpi`): rsmpi raw bindings
//!   (`crate::mpi_backend`); lifecycle and thread level in
//!   [`crate::lifecycle`].
//!
//! # Collective contract (D-039)
//!
//! Every collective carries a **descriptor** — operation, root, reduction
//! operator, element type, agreed element count (fixed-size collectives),
//! reduction policy — and a local validation status (root in range,
//! operator defined for the type, sizes representable). The descriptors of
//! all ranks are compared **before** any payload moves (MPI: one checked
//! header all-gather; local world: at rank 0): a mismatch or a failed local
//! validation on any rank makes the collective fail on **every** rank with
//! a recoverable error (each rank keeps its own local error), and the
//! communicator stays usable.
//!
//! # Reduction policy (D-039)
//!
//! [`ReductionPolicy::RankOrdered`] (default, every backend): reductions and
//! scans fold the gathered contributions in rank order
//! (`((v0 ∘ v1) ∘ v2)…`), deterministic for a fixed ordered set of rank
//! contributions and identical between serial, local world and MPI for a
//! fixed rank count. [`ReductionPolicy::NativeMpi`] (MPI only, chosen in
//! `MpiOptions`, inherited by duplicates and splits): MPI's typed predefined
//! reductions and scans, with upstream's blocking calls (`MPI_Allreduce`,
//! `MPI_Reduce`, `MPI_Scan`, `MPI_Exscan`; `MPI_MINLOC`/`MPI_MAXLOC` on
//! `MPI_DOUBLE_INT`) after the checked header — the association order is
//! the MPI implementation's; no cross-implementation bit identity is
//! promised. The policy is part of every collective's
//! descriptor, so ranks with different policies fail together.
//!
//! # Point-to-point requests
//!
//! Non-blocking operations own their buffers ([`SendRequest`],
//! [`RecvRequest`], [`BarrierRequest`]): a buffer cannot be freed or reused
//! while the backend may still access it. [`Communicator::irecv`] **posts**
//! the receive (a posted slot on every backend; MPI: a slot of the backend's
//! matching engine, filled by `MPI_Improbe` + an exact-length `MPI_Imrecv`,
//! never a fixed-size `MPI_Irecv`; messages to self go
//! through an in-process channel), so messages from one source with one tag
//! match posted receives in **posting order** and a posted receive lets a
//! synchronous send complete before the receive is waited on
//! (MPI: while the process waits in any akantu operation), as
//! upstream's `asyncReceive`. Blocking receives
//! ([`Communicator::receive_vec`]) and probes see only messages not matched
//! by a posted receive; the dynamic receive is a slot too, under one
//! operation deadline covering matching and payload.
//!
//! Dropping an incomplete request: a send or a barrier is completed
//! (bounded by the backend's deadline); a receive is **cancelled** — an
//! unmatched one is withdrawn; if a message had already been matched to it,
//! that message is consumed and discarded, as with `MPI_Cancel`. A completion failure
//! in a drop breaks the communicator's family; on an already broken
//! family the request is leaked (buffers and MPI guard kept, no MPI call):
//! the runtime aborts.

use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use akantu_core::{Error, FatalKind, Rank, Result};

use crate::buffer::{CommunicationBuffer, Packable};

/// Reduction operations (upstream `SynchronizerOperation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReduceOp {
    /// `_sum`
    Sum,
    /// `_min`
    Min,
    /// `_max`
    Max,
    /// `_prod`
    Prod,
    /// `_land`
    LogicalAnd,
    /// `_band`
    BitAnd,
    /// `_lor`
    LogicalOr,
    /// `_bor`
    BitOr,
    /// `_lxor`
    LogicalXor,
    /// `_bxor`
    BitXor,
    /// `_min_loc` (only for [`ValueLoc`])
    MinLoc,
    /// `_max_loc` (only for [`ValueLoc`])
    MaxLoc,
}

/// Message tag (upstream `Tag`: an integer built from a base tag and a
/// hash of the sender/receiver/synchronizer id, bounded by `MPI_TAG_UB`;
/// the MPI backend accepts the portable range `0..=32767`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag(pub i32);

/// Plain data that can be sent through a communicator (sealed). `usize` /
/// `isize` are deliberately excluded (no portable width); convert to
/// `u64`/`i64`.
pub trait CommData: Packable + Copy + Send + Sync + 'static + private::Sealed {}

mod private {
    use super::ReduceOp;
    use akantu_core::Result;

    pub trait Sealed: Sized {
        /// Wire type code, agreed in every collective descriptor.
        const CODE: u8;
        fn reduce(a: Self, b: Self, op: ReduceOp) -> Result<Self>;
        fn zero() -> Self;
        /// `true` if `op` is defined for this type (as MPI's predefined
        /// operations: arithmetic and min/max on numbers, logical and bit
        /// operations on integers, loc operations on value/location pairs).
        fn supports(op: ReduceOp) -> bool;
    }
}

/// Wire type code of `T` (collective descriptors, MPI datatype selection).
pub(crate) fn type_code<T: CommData>() -> u8 {
    <T as private::Sealed>::CODE
}

/// Name of a wire type code (diagnostics).
pub(crate) fn type_name(code: u64) -> &'static str {
    match code {
        0 => "untyped",
        1 => "u8",
        2 => "i8",
        3 => "i32",
        4 => "u32",
        5 => "i64",
        6 => "u64",
        7 => "f32",
        8 => "f64",
        9 => "ValueLoc",
        _ => "unknown",
    }
}

/// Reduction policy of a communicator (decision record D-039; see the
/// module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ReductionPolicy {
    /// Rank-ordered folds of the gathered contributions (every backend).
    #[default]
    RankOrdered,
    /// MPI's typed predefined reductions and scans (MPI backend only):
    /// implementation-defined association, for oracle qualification and
    /// performance comparison. Upstream's **blocking** calls: after their
    /// checked header they have **no progress deadline**
    /// (`MpiOptions::timeout` does not bound them), and under
    /// `MPI_THREAD_SERIALIZED` each one is an exclusive operation, refused
    /// with the same non-fatal `Error::Communication` on every rank while
    /// another exclusive operation (native reduction/scan or
    /// [`Communicator::split`]) is in flight in the process.
    NativeMpi,
}

impl ReductionPolicy {
    fn code(self) -> u64 {
        match self {
            Self::RankOrdered => 0,
            Self::NativeMpi => 1,
        }
    }
}

impl std::fmt::Display for ReductionPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RankOrdered => "rank-ordered",
            Self::NativeMpi => "native-mpi",
        })
    }
}

impl ReduceOp {
    /// Descriptor code (`1..=12`).
    pub(crate) fn code(self) -> u64 {
        match self {
            Self::Sum => 1,
            Self::Min => 2,
            Self::Max => 3,
            Self::Prod => 4,
            Self::LogicalAnd => 5,
            Self::BitAnd => 6,
            Self::LogicalOr => 7,
            Self::BitOr => 8,
            Self::LogicalXor => 9,
            Self::BitXor => 10,
            Self::MinLoc => 11,
            Self::MaxLoc => 12,
        }
    }

    fn name(code: u64) -> String {
        const ALL: [ReduceOp; 12] = [
            ReduceOp::Sum,
            ReduceOp::Min,
            ReduceOp::Max,
            ReduceOp::Prod,
            ReduceOp::LogicalAnd,
            ReduceOp::BitAnd,
            ReduceOp::LogicalOr,
            ReduceOp::BitOr,
            ReduceOp::LogicalXor,
            ReduceOp::BitXor,
            ReduceOp::MinLoc,
            ReduceOp::MaxLoc,
        ];
        ALL.iter()
            .find(|o| o.code() == code)
            .map_or_else(|| "none".to_owned(), |o| format!("{o:?}"))
    }
}

/// `a ∘ b` for a [`CommData`] value (accessors reducing received data).
pub(crate) fn reduce_value<T: CommData>(a: T, b: T, op: ReduceOp) -> Result<T> {
    <T as private::Sealed>::reduce(a, b, op)
}

fn unsupported(op: ReduceOp, ty: &str) -> Error {
    Error::InvalidArgument(format!("reduction {op:?} is not defined for {ty}"))
}

macro_rules! comm_int {
    ($($t:ty = $code:expr),*) => { $(
        impl private::Sealed for $t {
            const CODE: u8 = $code;
            fn supports(op: ReduceOp) -> bool {
                !matches!(op, ReduceOp::MinLoc | ReduceOp::MaxLoc)
            }
            fn reduce(a: Self, b: Self, op: ReduceOp) -> Result<Self> {
                Ok(match op {
                    ReduceOp::Sum => a.wrapping_add(b),
                    ReduceOp::Prod => a.wrapping_mul(b),
                    ReduceOp::Min => a.min(b),
                    ReduceOp::Max => a.max(b),
                    ReduceOp::LogicalAnd => Self::from((a != 0) && (b != 0)),
                    ReduceOp::LogicalOr => Self::from((a != 0) || (b != 0)),
                    ReduceOp::LogicalXor => Self::from((a != 0) != (b != 0)),
                    ReduceOp::BitAnd => a & b,
                    ReduceOp::BitOr => a | b,
                    ReduceOp::BitXor => a ^ b,
                    ReduceOp::MinLoc | ReduceOp::MaxLoc => {
                        return Err(unsupported(op, stringify!($t)))
                    }
                })
            }
            fn zero() -> Self { 0 }
        }
        impl CommData for $t {}
    )* };
}
comm_int!(u8 = 1, i8 = 2, i32 = 3, u32 = 4, i64 = 5, u64 = 6);

macro_rules! comm_float {
    ($($t:ty = $code:expr),*) => { $(
        impl private::Sealed for $t {
            const CODE: u8 = $code;
            fn supports(op: ReduceOp) -> bool {
                matches!(op, ReduceOp::Sum | ReduceOp::Prod | ReduceOp::Min | ReduceOp::Max)
            }
            fn reduce(a: Self, b: Self, op: ReduceOp) -> Result<Self> {
                Ok(match op {
                    ReduceOp::Sum => a + b,
                    ReduceOp::Prod => a * b,
                    ReduceOp::Min => if b < a { b } else { a },
                    ReduceOp::Max => if b > a { b } else { a },
                    _ => return Err(unsupported(op, stringify!($t))),
                })
            }
            fn zero() -> Self { 0.0 }
        }
        impl CommData for $t {}
    )* };
}
comm_float!(f32 = 7, f64 = 8);

/// Value with a location for `MinLoc`/`MaxLoc` reductions (upstream
/// `SCMinMaxLoc<Real, int>`, `MPI_DOUBLE_INT`). Ties keep the smallest
/// location (as `MPI_MINLOC`/`MPI_MAXLOC`); a NaN value never replaces the
/// accumulated one (the fold is rank-ordered, see the module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ValueLoc {
    /// The value compared.
    pub value: f64,
    /// Its location (rank, index, …).
    pub loc: i32,
}

impl Packable for ValueLoc {
    const SIZE: usize = 12;
    fn pack(&self, out: &mut Vec<u8>) {
        self.value.pack(out);
        self.loc.pack(out);
    }
    fn unpack(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(Error::Communication("buffer underflow".into()));
        }
        Ok(Self {
            value: f64::unpack(bytes)?,
            loc: i32::unpack(&bytes[8..])?,
        })
    }
}

impl private::Sealed for ValueLoc {
    const CODE: u8 = 9;
    fn supports(op: ReduceOp) -> bool {
        matches!(op, ReduceOp::MinLoc | ReduceOp::MaxLoc)
    }
    fn reduce(a: Self, b: Self, op: ReduceOp) -> Result<Self> {
        let tie = |a: Self, b: Self| Self {
            value: a.value,
            loc: a.loc.min(b.loc),
        };
        Ok(match op {
            ReduceOp::MinLoc if b.value < a.value => b,
            ReduceOp::MaxLoc if b.value > a.value => b,
            ReduceOp::MinLoc | ReduceOp::MaxLoc if b.value == a.value => tie(a, b),
            ReduceOp::MinLoc | ReduceOp::MaxLoc => a,
            _ => return Err(unsupported(op, "ValueLoc")),
        })
    }
    fn zero() -> Self {
        Self::default()
    }
}
impl CommData for ValueLoc {}

/// Status of a probed message (upstream `CommunicationStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// Sending rank.
    pub source: Rank,
    /// Message tag.
    pub tag: Tag,
    /// Number of items of the probed type.
    pub count: usize,
}

/// Exit code of [`Communicator::abort_on_transport_fatal`] (`EX_OSERR`).
pub const TRANSPORT_ABORT_CODE: i32 = 71;

/// Largest message (bytes) any backend accepts: MPI counts are C `int`s,
/// and every backend applies the same limit so that a size accepted in
/// local-world tests is accepted by MPI (sizes are
/// validated before anything is allocated or posted).
pub const MAX_MESSAGE_BYTES: usize = i32::MAX as usize;

/// `n` items of `item` bytes as a message size, checked against
/// [`MAX_MESSAGE_BYTES`] (never a huge allocation).
pub(crate) fn message_bytes(n: usize, item: usize, what: &str) -> Result<usize> {
    n.checked_mul(item)
        .filter(|&b| b <= MAX_MESSAGE_BYTES)
        .ok_or_else(|| {
            Error::InvalidArgument(format!(
                "{what}: {n} items of {item} bytes exceed the message limit of {MAX_MESSAGE_BYTES} bytes"
            ))
        })
}

// ------------------------------------------------------- collective contract

/// Number of descriptor words: operation, root, reduction operator, element
/// type, agreed element count, reduction policy.
pub(crate) const DESC_WORDS: usize = 6;

/// Agreed descriptor of one collective operation (D-039):
/// compared on every rank before any payload moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CollDesc {
    pub(crate) op: CollOp,
    /// Root of a rooted collective.
    pub(crate) root: Option<Rank>,
    /// Reduction operator of a reduction or scan.
    pub(crate) reduce: Option<ReduceOp>,
    /// Element type code (`0`: no typed payload).
    pub(crate) ty: u8,
    /// Element count that must be equal on every rank (fixed-size
    /// collectives).
    pub(crate) count: Option<u64>,
    pub(crate) policy: ReductionPolicy,
    /// Status of the collective error protocol carried in the header
    /// (D-126; `None`: not armed). Not part of [`CollDesc::words`]: the
    /// statuses are combined (worst severity, equal phase ids and digests),
    /// not compared for equality.
    pub(crate) status: Option<HeaderStatus>,
}

impl CollDesc {
    pub(crate) fn new(op: CollOp, policy: ReductionPolicy) -> Self {
        Self {
            op,
            root: None,
            reduce: None,
            ty: 0,
            count: None,
            policy,
            status: None,
        }
    }

    fn typed<T: CommData>(mut self, count: Option<usize>) -> Self {
        self.ty = type_code::<T>();
        self.count = count.map(|c| c as u64);
        self
    }

    fn rooted(mut self, root: Rank) -> Self {
        self.root = Some(root);
        self
    }

    fn reducing(mut self, op: ReduceOp) -> Self {
        self.reduce = Some(op);
        self
    }

    /// The descriptor as words (the wire form compared on every rank).
    pub(crate) fn words(&self) -> [u64; DESC_WORDS] {
        [
            self.op as u64,
            self.root.map_or(u64::MAX, |r| u64::from(r.cast_unsigned())),
            self.reduce.map_or(0, ReduceOp::code),
            u64::from(self.ty),
            self.count.unwrap_or(u64::MAX),
            self.policy.code(),
        ]
    }
}

fn coll_name(code: u64) -> String {
    const ALL: [CollOp; 12] = [
        CollOp::Barrier,
        CollOp::AllReduce,
        CollOp::Broadcast,
        CollOp::AllGather,
        CollOp::Scan,
        CollOp::ExclusiveScan,
        CollOp::Reduce,
        CollOp::Gather,
        CollOp::Scatter,
        CollOp::Split,
        CollOp::Duplicate,
        CollOp::Native,
    ];
    ALL.iter()
        .find(|o| **o as u64 == code)
        .map_or_else(|| format!("op {code}"), |o| format!("{o:?}"))
}

fn desc_field(k: usize, w: u64) -> String {
    match k {
        0 => coll_name(w),
        1 if w == u64::MAX => "none".to_owned(),
        #[allow(clippy::cast_possible_truncation)] // the word is a cast `Rank`
        1 => format!("{}", w as u32 as i32),
        2 => ReduceOp::name(w),
        3 => type_name(w).to_owned(),
        4 if w == u64::MAX => "any".to_owned(),
        4 => w.to_string(),
        5 if w == 0 => "rank-ordered".to_owned(),
        5 => "native-mpi".to_owned(),
        _ => w.to_string(),
    }
}

/// Number of header words of the protocol status (D-126): armed flag and
/// severity, phase id, digest.
pub(crate) const STATUS_WORDS: usize = 3;

/// The collective error protocol's status carried in the header of a checked
/// collective (D-126): a deferred agreement folded into the collective
/// ([`crate::phase::Phases::run_collective`]) or an agreement itself. Every
/// rank's statuses are combined before any payload moves: a failure
/// reported by any rank (or ranks at different phases) fails the collective
/// on every rank with the combined verdict, whatever the descriptors say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeaderStatus {
    /// `phase::Severity` as a number (0 = Ok).
    pub(crate) severity: u8,
    /// Phase id (FNV-1a of the qualified names folded since the last
    /// agreement).
    pub(crate) id: u64,
    /// Schedule digest folded likewise.
    pub(crate) digest: u64,
}

impl HeaderStatus {
    /// Wire form: `[0, 0, 0]` when not armed, else `[1 + severity, id,
    /// digest]`.
    pub(crate) fn words(s: Option<Self>) -> [u64; STATUS_WORDS] {
        s.map_or([0; STATUS_WORDS], |s| {
            [1 + u64::from(s.severity), s.id, s.digest]
        })
    }
}

/// Combined protocol status of one checked collective (every rank computes
/// the same).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StatusVerdict {
    /// Worst reported severity (0 = Ok).
    pub(crate) worst: u8,
    /// Every rank armed a status, with the same phase id.
    pub(crate) ids_match: bool,
    /// Every armed digest is the same.
    pub(crate) digests_match: bool,
}

/// The verdict of the ranks' header statuses (rank order): `None` when no
/// rank armed one, or every rank armed one reporting success with equal ids
/// and digests (the collective proceeds); otherwise the verdict that fails
/// the collective on every rank.
pub(crate) fn status_verdict(
    statuses: impl IntoIterator<Item = [u64; STATUS_WORDS]>,
) -> Option<StatusVerdict> {
    let all: Vec<[u64; STATUS_WORDS]> = statuses.into_iter().collect();
    let armed: Vec<&[u64; STATUS_WORDS]> = all.iter().filter(|w| w[0] != 0).collect();
    let first = armed.first()?;
    let worst = armed.iter().map(|w| w[0] - 1).max().unwrap_or(0);
    let ids_match = armed.len() == all.len() && armed.iter().all(|w| w[1] == first[1]);
    let digests_match = armed.iter().all(|w| w[2] == first[2]);
    let v = StatusVerdict {
        worst: u8::try_from(worst).unwrap_or(u8::MAX),
        ids_match,
        digests_match,
    };
    (v.worst != 0 || !ids_match || !digests_match).then_some(v)
}

thread_local! {
    /// A status armed by the current thread's live scope for the next
    /// checked collective on the communicator of the given identity.
    static ARMED: Cell<Option<(usize, HeaderStatus)>> = const { Cell::new(None) };
    /// The verdict of the last checked collective of this thread that a
    /// header status failed.
    static VERDICT: Cell<Option<StatusVerdict>> = const { Cell::new(None) };
}

/// Arm `status` for the next checked collective this thread issues on
/// `comm` (D-126).
pub(crate) fn arm_status(comm: &Communicator, status: HeaderStatus) {
    ARMED.with(|a| a.set(Some((comm.identity(), status))));
    VERDICT.with(|v| v.set(None));
}

/// Disarm; `true` if the status had not been consumed.
pub(crate) fn disarm_status() -> bool {
    ARMED.with(Cell::take).is_some()
}

/// The verdict that failed this thread's last armed collective, if any
/// (taken).
pub(crate) fn take_verdict() -> Option<StatusVerdict> {
    VERDICT.with(Cell::take)
}

pub(crate) fn set_verdict(v: StatusVerdict) {
    VERDICT.with(|c| c.set(Some(v)));
}

/// The error of a collective that a header status failed: the rank's own
/// local error when it has one, else a common description.
pub(crate) fn status_error(v: StatusVerdict, own: Option<Error>) -> Error {
    own.unwrap_or_else(|| {
        comm_err(if v.worst != 0 {
            "collective: the operation failed on another rank".to_owned()
        } else {
            "collective: ranks reached different phases of the operation".to_owned()
        })
    })
}

/// Compare the descriptors and local validation statuses of every rank
/// (rank order). Every rank evaluates the same entries, so every rank
/// reaches the same verdict: `Err` describes the first difference (the
/// operation itself, then root, operator, type, count, policy) or the first
/// rank whose local validation failed.
pub(crate) fn check_entries(
    entries: &[([u64; DESC_WORDS], bool)],
) -> std::result::Result<(), String> {
    const FIELDS: [&str; DESC_WORDS] = [
        "operation",
        "root",
        "reduction operator",
        "element type",
        "element count",
        "reduction policy",
    ];
    let Some((first, _)) = entries.first() else {
        return Ok(());
    };
    for (r, (w, _)) in entries.iter().enumerate() {
        if w[0] != first[0] {
            return Err(format!(
                "collective mismatch: rank 0 in {}, rank {r} in {}",
                desc_field(0, first[0]),
                desc_field(0, w[0])
            ));
        }
    }
    for k in 1..DESC_WORDS {
        if let Some(r) = entries.iter().position(|(w, _)| w[k] != first[k]) {
            return Err(format!(
                "collective {} contract mismatch: ranks disagree on the {} (rank 0: {}, rank {r}: {})",
                desc_field(0, first[0]),
                FIELDS[k],
                desc_field(k, first[k]),
                desc_field(k, entries[r].0[k])
            ));
        }
    }
    if let Some(r) = entries.iter().position(|(_, ok)| !ok) {
        return Err(format!(
            "collective {}: local validation failed on rank {r}",
            desc_field(0, first[0])
        ));
    }
    Ok(())
}

/// Local validation of a rank argument against `size`.
fn validate_rank(r: Rank, size: usize, what: &str) -> Result<()> {
    match usize::try_from(r) {
        Ok(u) if u < size => Ok(()),
        _ => Err(Error::InvalidArgument(format!(
            "{what}: rank {r} out of range (size {size})"
        ))),
    }
}

/// Local validation of a reduction operator for `T`.
fn validate_op<T: CommData>(op: ReduceOp) -> Result<()> {
    if <T as private::Sealed>::supports(op) {
        Ok(())
    } else {
        Err(unsupported(op, type_name(u64::from(type_code::<T>()))))
    }
}

// ------------------------------------------------------------------ backends

/// A queued point-to-point or collective message.
#[derive(Debug)]
struct Message {
    id: u64,
    bytes: Vec<u8>,
    /// Synchronous send: the sender waits for consumption.
    sync: bool,
}

/// Posted receives (serial backend and local world): a receive posted by
/// [`Communicator::irecv`] is a slot that the next matching message fills
/// in posting order.
#[derive(Debug)]
pub(crate) struct PostedSlots<K: std::hash::Hash + Eq> {
    /// Unfilled slots per matching key, in posting order.
    posted: HashMap<K, VecDeque<u64>>,
    /// Slot contents (`None` until matched).
    slots: HashMap<u64, Option<Vec<u8>>>,
    next: u64,
}

impl<K: std::hash::Hash + Eq> Default for PostedSlots<K> {
    fn default() -> Self {
        Self {
            posted: HashMap::new(),
            slots: HashMap::new(),
            next: 0,
        }
    }
}

impl<K: std::hash::Hash + Eq + Copy> PostedSlots<K> {
    /// Deliver `bytes` to the oldest posted slot of `key`; gives the bytes
    /// back if no receive is posted.
    pub(crate) fn deliver(&mut self, key: K, bytes: Vec<u8>) -> std::result::Result<(), Vec<u8>> {
        match self.posted.get_mut(&key).and_then(VecDeque::pop_front) {
            Some(slot) => {
                self.slots.insert(slot, Some(bytes));
                Ok(())
            }
            None => Err(bytes),
        }
    }

    /// Post a slot, already filled with `matched` if a message was waiting.
    pub(crate) fn post(&mut self, key: K, matched: Option<Vec<u8>>) -> u64 {
        let slot = self.next;
        self.next += 1;
        if matched.is_none() {
            self.posted.entry(key).or_default().push_back(slot);
        }
        self.slots.insert(slot, matched);
        slot
    }

    /// Take the contents of a filled slot (the slot is released).
    pub(crate) fn take(&mut self, slot: u64) -> Option<Vec<u8>> {
        if matches!(self.slots.get(&slot), Some(Some(_))) {
            self.slots.remove(&slot).flatten()
        } else {
            None
        }
    }

    /// Cancel slot `slot` (posted under `key`): an unmatched slot is
    /// withdrawn, a matched message is discarded.
    pub(crate) fn cancel(&mut self, key: K, slot: u64) {
        if let Some(q) = self.posted.get_mut(&key) {
            q.retain(|&s| s != slot);
            if q.is_empty() {
                self.posted.remove(&key);
            }
        }
        self.slots.remove(&slot);
    }
}

/// Serial backend state: self messages per tag and posted receives.
#[derive(Debug, Default)]
struct SerialState {
    queue: HashMap<Tag, VecDeque<Vec<u8>>>,
    posted: PostedSlots<Tag>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Channel {
    P2P(Tag),
    Collective(u64),
}

/// Collective operation codes carried in every collective message (local
/// world) or checked header (MPI) so that ranks entering **different**
/// collectives are detected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CollOp {
    Barrier = 1,
    AllReduce = 2,
    Broadcast = 3,
    AllGather = 4,
    Scan = 5,
    ExclusiveScan = 6,
    Reduce = 7,
    Gather = 8,
    Scatter = 9,
    Split = 10,
    Duplicate = 11,
    /// A blocking collective of a native library (MUMPS, PETSc) on the
    /// communicator's MPI handle ([`Communicator::native_collective`]).
    Native = 12,
}

/// Pending messages per `(source, destination, channel)`.
type Queues = HashMap<(Rank, Rank, Channel), VecDeque<Message>>;

/// Completion semantics of point-to-point sends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SendMode {
    /// Standard mode (upstream `CommunicationMode::_auto`): local world
    /// sends are buffered and complete when posted (small MPI messages
    /// below the eager limit); MPI uses `MPI_Isend`.
    #[default]
    Eager,
    /// Synchronous mode (upstream `CommunicationMode::_synchronous`,
    /// `MPI_Issend`): a send completes only when the matching receive
    /// consumed it (also the MPI rendezvous protocol for large messages).
    /// A blocking `send` waits for the receiver; an `isend` request
    /// completes on consumption.
    Rendezvous,
}

#[derive(Debug, Default)]
struct WorldState {
    queues: Queues,
    /// Ids of synchronous messages consumed by a receive and not yet
    /// acknowledged to their sender.
    consumed: HashSet<u64>,
    next_id: u64,
    /// Non-blocking barriers: arrivals and completion observations per
    /// collective sequence number.
    barriers: HashMap<u64, (usize, usize)>,
    /// Worlds created by a split (or duplicate), not yet picked up by all
    /// their members: `(sequence, color) -> (world, members left)`.
    children: HashMap<(u64, u32), (Arc<LocalWorld>, usize)>,
    /// Posted receives keyed by `(source, destination, tag)`.
    posted: PostedSlots<(Rank, Rank, Tag)>,
}

#[derive(Debug)]
struct LocalWorld {
    size: usize,
    state: Mutex<WorldState>,
    arrived: Condvar,
    timeout: Duration,
    mode: SendMode,
    /// Waits that ended at the world timeout, counted over this world and
    /// every world split or duplicated from it, on every rank
    /// ([`Communicator::local_timeouts`]).
    timeouts: Arc<AtomicU64>,
}

impl LocalWorld {
    /// Record a wait that ended at the timeout.
    fn timed_out(&self) {
        self.timeouts.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct LocalRank {
    rank: Rank,
    world: Arc<LocalWorld>,
    collective_seq: AtomicU64,
}

#[derive(Debug)]
enum Backend {
    /// Single process; point-to-point messages to self go through a mailbox.
    Serial { state: Mutex<SerialState> },
    /// One rank of an in-process world (threads).
    Local(LocalRank),
    /// One rank of an MPI communicator.
    #[cfg(feature = "mpi")]
    Mpi(crate::mpi_backend::MpiBackend),
}

type FinalizeHandler = Box<dyn FnOnce() + Send>;

/// Backend plus the communicator event handlers (upstream
/// `EventHandlerManager<CommunicatorEventHandler>`), shared by all clones of
/// one rank's handle.
struct Shared {
    backend: Backend,
    finalize: Mutex<Vec<FinalizeHandler>>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // Upstream `FinalizeCommunicatorEvent` (MPI-023): sent when the
        // communicator is destroyed, before its MPI handle is freed (the
        // `backend` field drops after this body), e.g. to destroy MUMPS
        // instances bound to it.
        let handlers = std::mem::take(
            &mut *self
                .finalize
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for h in handlers {
            h();
        }
    }
}

/// Communicator handle (cheap to clone; all clones share the backend and
/// therefore the collective sequence of their rank, and the registry of
/// active collective scopes).
#[derive(Clone, Debug)]
pub struct Communicator {
    shared: Arc<Shared>,
    scopes: Arc<ScopeRegistry>,
}

/// The live owner scope ([`crate::phase::Phases`]) of a communicator.
#[derive(Debug)]
struct ActiveScope {
    uid: u64,
    name: String,
    /// Thread that currently owns the scope: the thread that opened it or
    /// last resumed it (`None` while it is parked for an explicit transfer,
    /// [`crate::phase::ParkedScope`]).
    owner: Option<std::thread::ThreadId>,
    /// Nesting depth of the scope's own communication steps (agreements,
    /// `run`/`collective` closures): raw operations of the owner thread
    /// are allowed while > 0.
    comm_depth: u32,
}

/// Registry state of one rank's communicator (shared by all clones).
#[derive(Debug, Default)]
struct RegistryState {
    /// The live owner scope, if any (at most one per communicator).
    live: Option<ActiveScope>,
}

/// Transport-failure state shared by a communicator and every communicator
/// derived from it on this rank (duplicates, splits): a transport failure
/// observed on any of them means a peer may be blocked in an unmatched
/// operation, so none of them communicates again (the
/// runtime aborts).
#[derive(Debug)]
struct Family {
    broken: Mutex<Option<String>>,
    /// [`crate::phase::CollectiveChecks`] code of the family (D-126).
    checks: AtomicU8,
}

impl Default for Family {
    fn default() -> Self {
        // An invalid environment value is reported where creation can fail
        // (`Communicator::mpi_world` / `from_mpi`); elsewhere it keeps
        // every agreement (D-018's behaviour).
        let mode = crate::phase::default_collective_checks()
            .unwrap_or(crate::phase::CollectiveChecks::EveryPhase);
        Self {
            broken: Mutex::new(None),
            checks: AtomicU8::new(mode.code()),
        }
    }
}

/// Collective scopes of one rank's communicator.
///
/// At most **one live owner scope per communicator** — whatever the thread:
/// while one is live (running on its owner thread, or parked during an
/// explicit transfer), a second, independent [`crate::phase::Phases`]
/// opened on the same communicator from **any** thread is *refused* (it
/// never communicates; its operations return an error), and so is every
/// raw operation (collective or point-to-point) unless it is issued by the
/// scope's **owner thread inside one of the scope's communication steps**.
/// Both would otherwise turn a rank-specific failure into a mismatched
/// collective on the peers (a hang under MPI). The owner identity follows
/// the scope only through the checked transfer `Phases::park` /
/// `ParkedScope::resume` (a live `Phases` is `!Send + !Sync`). Code running
/// inside a collective operation communicates through the enclosing scope
/// it was handed (mesh consumers, solver callbacks via
/// `DofManager::phases`).
///
/// Every communicator has its own registry — the serial and local-world
/// handles, MPI communicators and every duplicate or split of them (an
/// independent message space with its own scopes; creating one is itself a
/// raw collective on the parent, refused while a parent scope is live).
/// The transport-failure state is shared with derived communicators
/// ([`Family`]).
///
/// The registry decides deterministically for everything that happens on
/// any thread **while** a scope is live on every rank; it cannot order
/// threads that race to open scopes on one communicator — issuing one
/// communicator's collective operations concurrently from several threads
/// violates MPI's ordering rule and remains a contract violation.
#[derive(Debug, Default)]
struct ScopeRegistry {
    state: Mutex<RegistryState>,
    next: AtomicU64,
    family: Arc<Family>,
}

impl ScopeRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, RegistryState> {
        // Nothing panics under this lock; a poisoned lock still holds a
        // consistent state.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn broken(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.family
            .broken
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Registry of a communicator derived from this one (same family).
    fn derived(&self) -> Self {
        Self {
            state: Mutex::default(),
            next: AtomicU64::new(0),
            family: Arc::clone(&self.family),
        }
    }
}

/// Why opening a scope was refused.
#[derive(Debug)]
pub(crate) enum OpenRefusal {
    /// Another scope (named) is live on the communicator.
    Active(String),
    /// The communicator observed a transport failure.
    Broken,
}

/// Guard of one communication step of an active scope (raw operations of
/// the owner thread are allowed while it lives).
#[derive(Debug)]
pub(crate) struct ScopeStep<'a> {
    registry: &'a ScopeRegistry,
    uid: Option<u64>,
}

impl Drop for ScopeStep<'_> {
    fn drop(&mut self) {
        if let Some(uid) = self.uid
            && let Some(s) = self.registry.lock().live.as_mut()
            && s.uid == uid
        {
            s.comm_depth = s.comm_depth.saturating_sub(1);
        }
    }
}

impl Default for Communicator {
    fn default() -> Self {
        Self::serial()
    }
}

fn comm_err(msg: impl Into<String>) -> Error {
    Error::Communication(msg.into())
}

impl LocalRank {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, WorldState>> {
        self.world
            .state
            .lock()
            .map_err(|_| comm_err("poisoned local world"))
    }

    /// Queue a message (a point-to-point message first fills the oldest
    /// receive posted for it); returns its id.
    fn push(&self, dst: Rank, ch: Channel, bytes: Vec<u8>, sync: bool) -> Result<u64> {
        let mut st = self.lock()?;
        let id = st.next_id;
        st.next_id += 1;
        let bytes = match ch {
            Channel::P2P(tag) => match st.posted.deliver((self.rank, dst, tag), bytes) {
                Ok(()) => {
                    // Matched by a posted receive: a synchronous send
                    // completes (MPI: the receive has started).
                    if sync || self.world.mode == SendMode::Rendezvous {
                        st.consumed.insert(id);
                    }
                    self.world.arrived.notify_all();
                    return Ok(id);
                }
                Err(bytes) => bytes,
            },
            Channel::Collective(_) => bytes,
        };
        st.queues
            .entry((self.rank, dst, ch))
            .or_default()
            .push_back(Message { id, bytes, sync });
        self.world.arrived.notify_all();
        Ok(id)
    }

    /// Take the next message of `(src, me, ch)` from a locked state.
    fn take(&self, st: &mut WorldState, src: Rank, ch: Channel) -> Option<Vec<u8>> {
        let m = st
            .queues
            .get_mut(&(src, self.rank, ch))
            .and_then(VecDeque::pop_front)?;
        if matches!(ch, Channel::P2P(_)) && (m.sync || self.world.mode == SendMode::Rendezvous) {
            st.consumed.insert(m.id);
            self.world.arrived.notify_all();
        }
        Some(m.bytes)
    }

    /// Post a receive from `src` with `tag` (matched at once if a message
    /// is waiting); returns its slot.
    fn post_recv(&self, src: Rank, tag: Tag) -> Result<u64> {
        let mut st = self.lock()?;
        let matched = self.take(&mut st, src, Channel::P2P(tag));
        Ok(st.posted.post((src, self.rank, tag), matched))
    }

    /// Wait (bounded) for posted receive `slot` to be matched; cancelled on
    /// timeout.
    fn wait_slot(&self, src: Rank, tag: Tag, slot: u64) -> Result<Vec<u8>> {
        let r = self.wait_for(&|| format!("message {tag:?} from rank {src}"), |st| {
            st.posted.take(slot)
        });
        if r.is_err() {
            self.cancel_slot(src, tag, slot);
        }
        r
    }

    fn cancel_slot(&self, src: Rank, tag: Tag, slot: u64) {
        if let Ok(mut st) = self.lock() {
            st.posted.cancel((src, self.rank, tag), slot);
        }
    }

    /// Wait (bounded by the world timeout) until `ready` returns a value.
    fn wait_for<T>(
        &self,
        what: &dyn Fn() -> String,
        mut ready: impl FnMut(&mut WorldState) -> Option<T>,
    ) -> Result<T> {
        let deadline = Instant::now() + self.world.timeout;
        let mut st = self.lock()?;
        loop {
            if let Some(v) = ready(&mut st) {
                return Ok(v);
            }
            let now = Instant::now();
            if now >= deadline {
                self.world.timed_out();
                return Err(comm_err(format!(
                    "rank {}: timed out after {:?} waiting for {}",
                    self.rank,
                    self.world.timeout,
                    what()
                )));
            }
            st = self
                .world
                .arrived
                .wait_timeout(st, deadline - now)
                .map_err(|_| comm_err("poisoned local world"))?
                .0;
        }
    }

    fn pop(&self, src: Rank, ch: Channel, what: &str) -> Result<Vec<u8>> {
        self.wait_for(&|| format!("{what} from rank {src}"), |st| {
            self.take(st, src, ch)
        })
    }

    /// Source and size of the oldest pending (unmatched) message with `tag`
    /// from `src` (`None`: any source).
    fn peek(&self, st: &WorldState, src: Option<Rank>, tag: Tag) -> Option<(Rank, usize)> {
        let ch = Channel::P2P(tag);
        let sources: Vec<Rank> = match src {
            Some(s) => vec![s],
            None => (0..self.world.size)
                .filter_map(|r| Rank::try_from(r).ok())
                .collect(),
        };
        sources
            .into_iter()
            .filter_map(|s| {
                st.queues
                    .get(&(s, self.rank, ch))
                    .and_then(VecDeque::front)
                    .map(|m| (m.id, s, m.bytes.len()))
            })
            .min()
            .map(|(_, s, n)| (s, n))
    }

    /// `true` if synchronous message `id` was consumed (acknowledges it).
    fn take_consumed(&self, id: u64) -> Result<bool> {
        Ok(self.lock()?.consumed.remove(&id))
    }

    /// Wait until synchronous message `id` was consumed by its receiver.
    fn wait_consumed(&self, id: u64, dst: Rank) -> Result<()> {
        self.wait_for(
            &|| format!("rank {dst} to receive a rendezvous send"),
            |st| st.consumed.remove(&id).then_some(()),
        )
    }

    fn next_seq(&self) -> u64 {
        self.collective_seq.fetch_add(1, Ordering::SeqCst)
    }

    /// One checked collective: every rank sends its
    /// descriptor, validation status and payload to rank 0, which compares
    /// the descriptors and — only if they agree and every rank validated
    /// its input — computes every rank's result with `compute`; otherwise
    /// (or if `compute` fails) every rank receives the same error. A rank
    /// returns its own local error in preference.
    fn collective(
        &self,
        desc: &CollDesc,
        local: Result<()>,
        payload: Vec<u8>,
        compute: impl FnOnce(Vec<Vec<u8>>) -> Result<Vec<Vec<u8>>>,
    ) -> Result<Vec<u8>> {
        const HEAD: usize = (DESC_WORDS + STATUS_WORDS) * 8 + 1;
        let seq = self.next_seq();
        let ch = Channel::Collective(seq);
        let mut msg = Vec::with_capacity(HEAD + payload.len());
        for w in desc
            .words()
            .into_iter()
            .chain(HeaderStatus::words(desc.status))
        {
            msg.extend_from_slice(&w.to_le_bytes());
        }
        msg.push(u8::from(local.is_ok()));
        msg.extend(payload);
        let what = format!("collective {:?} #{seq}", desc.op);
        let reply = if self.rank != 0 {
            self.push(0, ch, msg, false)?;
            self.pop(0, ch, &format!("{what} result"))?
        } else {
            let mut entries = Vec::with_capacity(self.world.size);
            let mut statuses = Vec::with_capacity(self.world.size);
            let mut payloads = Vec::with_capacity(self.world.size);
            let mut parse = |m: Vec<u8>| -> Result<()> {
                if m.len() < HEAD {
                    return Err(comm_err("malformed collective message"));
                }
                let word = |k: usize| {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&m[8 * k..8 * k + 8]);
                    u64::from_le_bytes(b)
                };
                let mut words = [0u64; DESC_WORDS];
                for (k, w) in words.iter_mut().enumerate() {
                    *w = word(k);
                }
                let mut st = [0u64; STATUS_WORDS];
                for (k, w) in st.iter_mut().enumerate() {
                    *w = word(DESC_WORDS + k);
                }
                entries.push((words, m[HEAD - 1] != 0));
                statuses.push(st);
                payloads.push(m[HEAD..].to_vec());
                Ok(())
            };
            parse(msg)?;
            for r in 1..self.world.size {
                let r = Rank::try_from(r).map_err(|_| comm_err("rank overflow"))?;
                parse(self.pop(r, ch, &what)?)?;
            }
            // D-126: a protocol status reported by any rank decides first
            // (whatever the descriptors), no payload is combined
            let verdict = status_verdict(statuses);
            let outcome = match verdict {
                Some(_) => Err(String::new()),
                None => check_entries(&entries)
                    .and_then(|()| compute(payloads).map_err(|e| e.to_string())),
            };
            let replies: Vec<Vec<u8>> = match (verdict, outcome) {
                (Some(v), _) => {
                    vec![
                        vec![2, v.worst, u8::from(v.ids_match), u8::from(v.digests_match)];
                        self.world.size
                    ]
                }
                (None, outcome) => match outcome {
                    Ok(results) if results.len() == self.world.size => results
                        .into_iter()
                        .map(|mut b| {
                            b.insert(0, 1);
                            b
                        })
                        .collect(),
                    Ok(_) => {
                        vec![b"\0internal error: wrong number of results".to_vec(); self.world.size]
                    }
                    Err(e) => {
                        let mut m = vec![0u8];
                        m.extend_from_slice(e.as_bytes());
                        vec![m; self.world.size]
                    }
                },
            };
            let mut replies = replies.into_iter();
            let own = replies.next().unwrap_or_default();
            for (r, m) in replies.enumerate() {
                let r = Rank::try_from(r + 1).map_err(|_| comm_err("rank overflow"))?;
                self.push(r, ch, m, false)?;
            }
            own
        };
        match reply.split_first() {
            Some((1, rest)) => local.map(|()| rest.to_vec()),
            Some((2, rest)) if rest.len() == 3 => {
                let v = StatusVerdict {
                    worst: rest[0],
                    ids_match: rest[1] != 0,
                    digests_match: rest[2] != 0,
                };
                set_verdict(v);
                Err(status_error(v, local.err()))
            }
            Some((_, rest)) => Err(local
                .err()
                .unwrap_or_else(|| comm_err(String::from_utf8_lossy(rest).into_owned()))),
            None => Err(comm_err("empty collective reply")),
        }
    }
}

fn to_bytes<T: CommData>(values: &[T]) -> Vec<u8> {
    let mut buf = CommunicationBuffer::with_capacity(values.len() * T::SIZE);
    buf.pack_slice(values);
    buf.into_bytes()
}

fn from_bytes<T: CommData>(bytes: &[u8], out: &mut [T]) -> Result<()> {
    if bytes.len() != out.len() * T::SIZE {
        return Err(comm_err(format!(
            "message size mismatch: received {} bytes, expected {}",
            bytes.len(),
            out.len() * T::SIZE
        )));
    }
    T::unpack_all(bytes, out)
}

/// `values` as its bytes, without a copy, when `T` is `u8` (the packed
/// buffers of the synchronizers); `Err(values)` otherwise.
#[cfg(feature = "mpi")]
fn byte_vec<T: CommData>(values: Vec<T>) -> std::result::Result<Vec<u8>, Vec<T>> {
    let mut slot = Some(values);
    if let Some(bytes) = (&mut slot as &mut dyn std::any::Any).downcast_mut::<Option<Vec<u8>>>() {
        return Ok(bytes.take().unwrap_or_default());
    }
    Err(slot.unwrap_or_default())
}

/// `bytes` as a `Vec<T>`, without a copy, when `T` is `u8`.
fn from_byte_vec<T: CommData>(bytes: Vec<u8>) -> std::result::Result<Vec<T>, Vec<u8>> {
    let mut slot = Some(bytes);
    if let Some(v) = (&mut slot as &mut dyn std::any::Any).downcast_mut::<Option<Vec<T>>>() {
        return Ok(v.take().unwrap_or_default());
    }
    Err(slot.unwrap_or_default())
}

fn vec_from_bytes<T: CommData>(bytes: &[u8]) -> Result<Vec<T>> {
    if !bytes.len().is_multiple_of(T::SIZE) {
        return Err(comm_err("message size is not a multiple of the item size"));
    }
    let mut v = vec![T::zero(); bytes.len() / T::SIZE];
    from_bytes(bytes, &mut v)?;
    Ok(v)
}

/// Pack per-rank byte blocks with a length table (local-world transport of
/// variable-size collectives).
fn pack_blocks(blocks: &[Vec<u8>]) -> Vec<u8> {
    let mut packed = CommunicationBuffer::new();
    packed.pack(&u64::try_from(blocks.len()).unwrap_or(u64::MAX));
    for b in blocks {
        packed.pack(&u64::try_from(b.len()).unwrap_or(u64::MAX));
    }
    let mut bytes = packed.into_bytes();
    for b in blocks {
        bytes.extend_from_slice(b);
    }
    bytes
}

fn unpack_blocks(bytes: Vec<u8>) -> Result<Vec<Vec<u8>>> {
    let mut buf = CommunicationBuffer::from_bytes(bytes);
    let n = usize::try_from(buf.unpack::<u64>()?)
        .ok()
        .filter(|&n| n <= buf.remaining() / 8)
        .ok_or_else(|| comm_err("block count"))?;
    let mut lens = Vec::with_capacity(n);
    for _ in 0..n {
        lens.push(usize::try_from(buf.unpack::<u64>()?).map_err(|_| comm_err("block length"))?);
    }
    let mut out = Vec::with_capacity(n);
    for len in lens {
        if len > buf.remaining() {
            return Err(comm_err("block length exceeds the packed data"));
        }
        let mut chunk = vec![0u8; len];
        buf.unpack_slice(&mut chunk)?;
        out.push(chunk);
    }
    Ok(out)
}

/// Fold per-rank contributions in rank order: `((v0 ∘ v1) ∘ v2)…`.
fn fold_in_rank_order<T: CommData>(contributions: &[Vec<u8>], op: ReduceOp) -> Result<Vec<T>> {
    let mut iter = contributions.iter();
    let mut acc: Vec<T> = match iter.next() {
        Some(b) => vec_from_bytes(b)?,
        None => return Ok(Vec::new()),
    };
    for bytes in iter {
        let other: Vec<T> = vec_from_bytes(bytes)?;
        if other.len() != acc.len() {
            return Err(comm_err("reduction length mismatch between ranks"));
        }
        for (a, b) in acc.iter_mut().zip(other) {
            *a = T::reduce(*a, b, op)?;
        }
    }
    Ok(acc)
}

/// Rank-ordered prefix folds: entry `r` is `v0 ∘ … ∘ vr` (inclusive) or
/// `v0 ∘ … ∘ v(r-1)` with zero for rank 0 (exclusive).
fn prefix_folds<T: CommData>(
    contributions: &[Vec<u8>],
    op: ReduceOp,
    exclusive: bool,
) -> Result<Vec<Vec<T>>> {
    let mut out = Vec::with_capacity(contributions.len());
    let mut acc: Option<Vec<T>> = None;
    for bytes in contributions {
        let v: Vec<T> = vec_from_bytes(bytes)?;
        if exclusive {
            out.push(acc.clone().unwrap_or_else(|| vec![T::zero(); v.len()]));
        }
        acc = Some(match acc {
            None => v,
            Some(mut a) => {
                if a.len() != v.len() {
                    return Err(comm_err("scan length mismatch between ranks"));
                }
                for (x, y) in a.iter_mut().zip(v) {
                    *x = T::reduce(*x, y, op)?;
                }
                a
            }
        });
        if !exclusive {
            out.push(acc.clone().unwrap_or_default());
        }
    }
    Ok(out)
}

fn rank_index(r: Rank) -> Result<usize> {
    usize::try_from(r).map_err(|_| comm_err(format!("invalid rank {r}")))
}

impl Communicator {
    fn from_backend(backend: Backend, scopes: ScopeRegistry) -> Self {
        Self {
            shared: Arc::new(Shared {
                backend,
                finalize: Mutex::new(Vec::new()),
            }),
            scopes: Arc::new(scopes),
        }
    }

    fn backend(&self) -> &Backend {
        &self.shared.backend
    }

    /// Serial (single-rank) communicator — upstream `CommunicatorDummy`.
    #[must_use]
    pub fn serial() -> Self {
        Self::from_backend(
            Backend::Serial {
                state: Mutex::new(SerialState::default()),
            },
            ScopeRegistry::default(),
        )
    }

    /// Handles of an in-process world of `size` ranks (index = rank) with
    /// eager sends. Each handle must be moved to its own thread; see
    /// [`run_local`].
    #[must_use]
    pub fn local_world(size: usize, timeout: Duration) -> Vec<Self> {
        Self::local_world_with_mode(size, timeout, SendMode::Eager)
    }

    /// Handles of an in-process world with the given send semantics.
    #[must_use]
    pub fn local_world_with_mode(size: usize, timeout: Duration, mode: SendMode) -> Vec<Self> {
        let world = Arc::new(LocalWorld {
            size,
            state: Mutex::new(WorldState::default()),
            arrived: Condvar::new(),
            timeout,
            mode,
            timeouts: Arc::new(AtomicU64::new(0)),
        });
        (0..size)
            .map(|r| {
                Self::from_backend(
                    Backend::Local(LocalRank {
                        rank: Rank::try_from(r).unwrap_or(Rank::MAX),
                        world: Arc::clone(&world),
                        collective_seq: AtomicU64::new(0),
                    }),
                    ScopeRegistry::default(),
                )
            })
            .collect()
    }

    /// A duplicate of `MPI_COMM_WORLD` (upstream
    /// `Communicator::getStaticCommunicator`, MPI-001/002). MPI is
    /// initialised by akantu if needed (`MPI_Init_thread(required)`,
    /// finalised after the last guard, `docs/ARCHITECTURE.md`) or **imported** if
    /// the host (mpi4py, an application) initialised it first — then akantu
    /// never finalises it. The duplicate isolates akantu's traffic and uses
    /// `MPI_ERRORS_RETURN`.
    ///
    /// **Communicator creation under `MPI_THREAD_SERIALIZED`**
    /// (`docs/ARCHITECTURE.md`): every creation (split,
    /// duplication, world adoption, import) and every native-policy
    /// reduction takes part in one per-process admission rule
    /// (`mpi_backend::ExclusiveGuard`) that keeps Open MPI's blocking
    /// `MPI_Comm_split` from deadlocking across ranks with another thread's
    /// creation. Refusals are agreed, non-fatal `Error::Communication`s and
    /// the communicator stays usable, but there is **no fairness**: under
    /// contention from several threads a creation can be refused
    /// repeatedly — serialise creations per process, or retry with
    /// randomised back-off. Under `Multiple`, `Single` and `Funneled` the
    /// rule does not apply.
    /// World adoption has no header to agree on a refusal: under
    /// `Serialized` it **waits** (without the MPI lock, bounded by
    /// [`crate::MpiOptions::timeout`]) until no committed split is in
    /// flight in the process, then duplicates with a polled
    /// `MPI_Comm_idup` (its own deadline: the total can reach about twice
    /// the timeout). A timeout of the admission wait is
    /// `Error::Fatal { kind: Transport }`: peers may already have started
    /// their duplication, which can then only leak — abort the job, never
    /// retry.
    #[cfg(feature = "mpi")]
    pub fn mpi_world(options: crate::mpi_backend::MpiOptions) -> Result<Self> {
        let b = crate::mpi_backend::MpiBackend::world(options)?;
        Ok(Self::from_backend(
            Backend::Mpi(b),
            ScopeRegistry::default(),
        ))
    }

    /// Import a host **intra**communicator (an rsmpi `SimpleCommunicator`):
    /// akantu works on an `MPI_Comm_dup` of it (freed with akantu's last
    /// handle), never on the host's handle. Intercommunicators are rejected
    /// at compile time by the parameter type and at run time
    /// (`MPI_Comm_test_inter`) before any collective is issued;
    /// so is `MPI_COMM_NULL`.
    ///
    /// **Communicator creation under `MPI_THREAD_SERIALIZED`**
    /// (`docs/ARCHITECTURE.md`): every creation (split,
    /// duplication, world adoption, import) and every native-policy
    /// reduction takes part in one per-process admission rule
    /// (`mpi_backend::ExclusiveGuard`) that keeps Open MPI's blocking
    /// `MPI_Comm_split` from deadlocking across ranks with another thread's
    /// creation. Refusals are agreed, non-fatal `Error::Communication`s and
    /// the communicator stays usable, but there is **no fairness**: under
    /// contention from several threads a creation can be refused
    /// repeatedly — serialise creations per process, or retry with
    /// randomised back-off. Under `Multiple`, `Single` and `Funneled` the
    /// rule does not apply.
    /// Like [`Communicator::mpi_world`], an import waits for a committed
    /// split under `Serialized`, and a timeout of that wait is
    /// `Error::Fatal { kind: Transport }`.
    ///
    /// **MPI finalised by the host**: the host must not
    /// finalise MPI while akantu communicators are alive. An import after
    /// the host finalised MPI is refused with `Error::Communication` before
    /// any MPI call (also when an earlier import is still alive), and the
    /// destructions of akantu handles released afterwards are dropped
    /// without running (D-067 (1)); any other operation on a communicator
    /// that outlives the host's `MPI_Finalize` is an MPI call after
    /// `MPI_Finalize`, which MPI forbids and akantu cannot intercept.
    ///
    /// ```compile_fail
    /// # fn f(inter: &mpi::topology::InterCommunicator) {
    /// // An intercommunicator is not accepted.
    /// let _ = akantu_comm::Communicator::from_mpi(inter, Default::default());
    /// # }
    /// ```
    #[cfg(feature = "mpi")]
    #[allow(unsafe_code)] // borrowed-handle import of a live rsmpi communicator
    pub fn from_mpi(
        comm: &mpi::topology::SimpleCommunicator,
        options: crate::mpi_backend::MpiOptions,
    ) -> Result<Self> {
        use mpi::raw::AsRaw;
        // SAFETY: `comm` is a live rsmpi communicator, borrowed (not freed)
        // for the duration of the call.
        let b = unsafe { crate::mpi_backend::MpiBackend::import(comm.as_raw(), options)? };
        Ok(Self::from_backend(
            Backend::Mpi(b),
            ScopeRegistry::default(),
        ))
    }

    /// Import a host communicator given by its raw handle (e.g. the
    /// `MPI_Comm` behind an `mpi4py.MPI.Comm`), **borrowed** for the
    /// duration of the call: akantu duplicates it and never frees it.
    /// `MPI_COMM_NULL` and intercommunicators are rejected with an error
    /// before any collective is issued.
    ///
    /// **Communicator creation under `MPI_THREAD_SERIALIZED`**
    /// (`docs/ARCHITECTURE.md`): every creation (split,
    /// duplication, world adoption, import) and every native-policy
    /// reduction takes part in one per-process admission rule
    /// (`mpi_backend::ExclusiveGuard`) that keeps Open MPI's blocking
    /// `MPI_Comm_split` from deadlocking across ranks with another thread's
    /// creation. Refusals are agreed, non-fatal `Error::Communication`s and
    /// the communicator stays usable, but there is **no fairness**: under
    /// contention from several threads a creation can be refused
    /// repeatedly — serialise creations per process, or retry with
    /// randomised back-off. Under `Multiple`, `Single` and `Funneled` the
    /// rule does not apply.
    /// Like [`Communicator::mpi_world`], an import waits for a committed
    /// split under `Serialized`, and a timeout of that wait is
    /// `Error::Fatal { kind: Transport }`.
    ///
    /// **MPI finalised by the host**: the host must not
    /// finalise MPI while akantu communicators are alive. An import after
    /// the host finalised MPI is refused with `Error::Communication` before
    /// any MPI call (also when an earlier import is still alive), and the
    /// destructions of akantu handles released afterwards are dropped
    /// without running (D-067 (1)); any other operation on a communicator
    /// that outlives the host's `MPI_Finalize` is an MPI call after
    /// `MPI_Finalize`, which MPI forbids and akantu cannot intercept.
    ///
    /// # Safety
    ///
    /// `raw` must be `MPI_COMM_NULL` or a valid communicator handle of the
    /// running MPI environment that no other thread frees during the call.
    /// Errors raised by MPI while validating or duplicating the host handle
    /// are handled by the host communicator's own error handler (akantu
    /// installs `MPI_ERRORS_RETURN` on its duplicate only).
    #[cfg(feature = "mpi")]
    #[allow(unsafe_code)] // raw-handle import: the caller vouches for the handle (# Safety)
    pub unsafe fn from_raw_mpi(
        raw: mpi::ffi::MPI_Comm,
        options: crate::mpi_backend::MpiOptions,
    ) -> Result<Self> {
        // SAFETY: forwarded from this function's contract.
        let b = unsafe { crate::mpi_backend::MpiBackend::import(raw, options)? };
        Ok(Self::from_backend(
            Backend::Mpi(b),
            ScopeRegistry::default(),
        ))
    }

    /// Reduction policy of this communicator (D-039): always
    /// [`ReductionPolicy::RankOrdered`] for the serial backend and the local
    /// world; `MpiOptions::reduction` for MPI (inherited by duplicates and
    /// splits). Record it in fixture and run provenance.
    #[must_use]
    pub fn reduction_policy(&self) -> ReductionPolicy {
        match self.backend() {
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => m.options().reduction,
            _ => ReductionPolicy::RankOrdered,
        }
    }

    fn desc(&self, op: CollOp) -> CollDesc {
        let mut d = CollDesc::new(op, self.reduction_policy());
        let me = self.identity();
        d.status = ARMED.with(|a| match a.get() {
            Some((id, s)) if id == me => {
                a.set(None);
                Some(s)
            }
            _ => None,
        });
        d
    }

    /// The collective-check mode of this communicator's family (D-126).
    #[must_use]
    pub fn collective_checks(&self) -> crate::phase::CollectiveChecks {
        crate::phase::CollectiveChecks::from_code(self.scopes.family.checks.load(Ordering::Relaxed))
    }

    /// Set the collective-check mode of this communicator's family
    /// (duplicates and splits share it). Call it on every rank, outside any
    /// collective operation: ranks in different modes issue agreements at
    /// different points.
    pub fn set_collective_checks(&self, mode: crate::phase::CollectiveChecks) {
        self.scopes
            .family
            .checks
            .store(mode.code(), Ordering::Relaxed);
    }

    /// Register an owner scope named `name`, owned by the current thread.
    /// Refused if a scope is already live on this communicator — on any
    /// thread, also parked (an independent nested or concurrent scope),
    /// or if the communicator is broken.
    pub(crate) fn open_scope(&self, name: &str) -> std::result::Result<u64, OpenRefusal> {
        if self.scopes.broken().is_some() {
            return Err(OpenRefusal::Broken);
        }
        let mut st = self.scopes.lock();
        if let Some(s) = &st.live {
            return Err(OpenRefusal::Active(s.name.clone()));
        }
        let uid = self.scopes.next.fetch_add(1, Ordering::Relaxed);
        st.live = Some(ActiveScope {
            uid,
            name: name.to_owned(),
            owner: Some(std::thread::current().id()),
            comm_depth: 0,
        });
        Ok(uid)
    }

    /// Unregister scope `uid`.
    pub(crate) fn close_scope(&self, uid: u64) {
        let mut st = self.scopes.lock();
        if st.live.as_ref().is_some_and(|s| s.uid == uid) {
            st.live = None;
        }
    }

    /// Park scope `uid` for an explicit transfer: it stays live (admission
    /// keeps refusing other scopes and raw operations on every thread) but
    /// has no owner thread until resumed.
    pub(crate) fn park_scope(&self, uid: u64) {
        let mut st = self.scopes.lock();
        if let Some(s) = st.live.as_mut()
            && s.uid == uid
        {
            debug_assert_eq!(s.comm_depth, 0, "a scope is parked only between steps");
            s.owner = None;
        }
    }

    /// Resume parked scope `uid` on the current thread, which becomes its
    /// owner (the checked half of the transfer). Fails if `uid` is not the
    /// live, parked scope of this communicator.
    pub(crate) fn resume_scope(&self, uid: u64) -> Result<()> {
        let me = std::thread::current().id();
        let mut st = self.scopes.lock();
        match st.live.as_mut() {
            Some(s) if s.uid == uid && s.owner.is_none() => {
                s.owner = Some(me);
                Ok(())
            }
            Some(s) if s.uid == uid => Err(comm_err(format!(
                "collective scope '{}' resumed while owned by another thread",
                s.name
            ))),
            _ => Err(comm_err(
                "collective scope resumed after it was unregistered",
            )),
        }
    }

    /// Enter a communication step of scope `uid` (`None`: an unregistered
    /// scope, no effect).
    pub(crate) fn scope_step(&self, uid: Option<u64>) -> ScopeStep<'_> {
        if let Some(uid) = uid
            && let Some(s) = self.scopes.lock().live.as_mut()
            && s.uid == uid
        {
            debug_assert_eq!(s.owner, Some(std::thread::current().id()));
            s.comm_depth += 1;
        }
        ScopeStep {
            registry: &self.scopes,
            uid,
        }
    }

    /// Mark the communicator broken after a transport failure observed
    /// through one of its scopes (or a transport-fatal result reported to
    /// one, or a failed MPI call): every later scope is born broken and
    /// every raw operation is refused with `Error::Fatal { kind: Transport
    /// }` — nothing is communicated on it, nor on any communicator derived
    /// from it on this rank, again (the runtime aborts). The
    /// first reason is kept.
    pub(crate) fn mark_broken(&self, reason: &str) {
        let mut b = self.scopes.broken();
        if b.is_none() {
            *b = Some(reason.to_owned());
        }
    }

    /// The transport failure that broke this communicator (or a
    /// communicator it shares its family with), if any (local information:
    /// only ranks that observed it know).
    #[must_use]
    pub fn broken(&self) -> Option<String> {
        self.scopes.broken().clone()
    }

    /// Name of the collective scope live on this communicator, if any —
    /// whichever thread owns it, also while parked.
    #[must_use]
    pub fn active_scope(&self) -> Option<String> {
        self.scopes.lock().live.as_ref().map(|s| s.name.clone())
    }

    /// `true` if the live scope of this communicator is owned by the
    /// current thread.
    #[must_use]
    pub fn owns_active_scope(&self) -> bool {
        let me = std::thread::current().id();
        self.scopes
            .lock()
            .live
            .as_ref()
            .is_some_and(|s| s.owner == Some(me))
    }

    /// Raw operations are refused while a scope is live on this
    /// communicator, except on the scope's owner thread inside one of its
    /// communication steps: the error is local and nothing
    /// is communicated, so the enclosing scope's next agreement reports it
    /// on every rank. On a broken communicator every raw operation is
    /// refused with `Error::Fatal { kind: Transport }`.
    pub(crate) fn check_raw(&self, what: &str) -> Result<()> {
        if let Some(reason) = &*self.scopes.broken() {
            return Err(Error::fatal(
                FatalKind::Transport,
                comm_err(format!(
                    "raw {what} refused: the communicator observed a transport failure ({reason})"
                )),
            ));
        }
        let me = std::thread::current().id();
        let st = self.scopes.lock();
        match &st.live {
            Some(s) if s.owner == Some(me) && s.comm_depth > 0 => Ok(()),
            Some(s) => Err(comm_err(format!(
                "raw {what} refused: collective scope '{}' is active on this communicator{}; \
                 communicate through that scope's phases (mesh consumers: the Phases \
                 argument; solver callbacks: DofManager::phases)",
                s.name,
                match s.owner {
                    Some(o) if o == me => "",
                    Some(_) => " (owned by another thread)",
                    None => " (parked for a transfer)",
                }
            ))),
            None => Ok(()),
        }
    }

    /// Pass an MPI backend result through: a transport-fatal error (a
    /// failed MPI call or a deadline) marks the communicator broken.
    #[cfg(feature = "mpi")]
    fn mpi<R>(&self, r: Result<R>) -> Result<R> {
        if let Err(e) = &r
            && e.fatal_kind() == Some(FatalKind::Transport)
        {
            self.mark_broken(&e.to_string());
        }
        r
    }

    /// Rank of this process.
    #[must_use]
    pub fn rank(&self) -> Rank {
        match self.backend() {
            Backend::Serial { .. } => 0,
            Backend::Local(l) => l.rank,
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => m.rank(),
        }
    }

    /// Number of processes.
    #[must_use]
    pub fn size(&self) -> usize {
        match self.backend() {
            Backend::Serial { .. } => 1,
            Backend::Local(l) => l.world.size,
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => m.size(),
        }
    }

    /// `true` for single-rank communicators.
    #[must_use]
    pub fn is_serial(&self) -> bool {
        self.size() == 1
    }

    /// `true` for an MPI communicator.
    #[must_use]
    pub fn is_mpi(&self) -> bool {
        match self.backend() {
            #[cfg(feature = "mpi")]
            Backend::Mpi(_) => true,
            _ => false,
        }
    }

    /// Whether the calling thread may issue MPI calls on this communicator
    /// (its MPI environment's thread policy, D-023: under `Single` /
    /// `Funneled` only MPI's main thread may). Local and serial
    /// communicators always may. A purely local check that issues no MPI
    /// call, for callers that must refuse a call cleanly **before** a
    /// collective protocol starts (a refusal met inside one breaks the
    /// communicator, ARCHITECTURE.md, "Agreement across ranks"; the Python module's `Mesh.distribute()`).
    ///
    /// # Errors
    /// `Error::Communication` when the thread policy forbids the call.
    pub fn check_thread(&self) -> Result<()> {
        match self.backend() {
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => m.policy().check(),
            _ => Ok(()),
        }
    }

    /// `true` if `self` and `other` are handles of **the same**
    /// communicator: clones of one another, sharing the backend (for MPI:
    /// the same communicator handle, group, rank order and collective
    /// sequence). A duplicate or a split is a different communicator even
    /// when it is congruent (same processes, same order). Purely local; a
    /// rank-local verdict that callers needing a common decision must agree
    /// (a native object bound to one communicator must
    /// not be driven through another).
    #[must_use]
    pub fn is_same(&self, other: &Communicator) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }

    /// Identity of this rank's communicator (its shared state; equal for
    /// clones, [`Communicator::is_same`]), carried by native capabilities
    /// and by armed header statuses (D-126).
    pub(crate) fn identity(&self) -> usize {
        Arc::as_ptr(&self.shared) as usize
    }

    /// Progress deadline of blocking waits (local world: its timeout; MPI:
    /// `MpiOptions::timeout`; serial: none needed).
    #[must_use]
    pub fn progress_timeout(&self) -> Option<Duration> {
        match self.backend() {
            Backend::Serial { .. } => None,
            Backend::Local(l) => Some(l.world.timeout),
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => m.options().timeout,
        }
    }

    /// Local world only (`None` otherwise): how many waits ended at the
    /// world's timeout so far, on any rank of this world or of a world split
    /// or duplicated from it (a timeout error, whatever the caller then
    /// returned: a rank may prefer its own error to the timeout). Lets a test
    /// state "no rank waited for a timeout" as a count instead of as a
    /// wall-clock bound, which a stalled process would also exceed.
    #[must_use]
    pub fn local_timeouts(&self) -> Option<u64> {
        match self.backend() {
            Backend::Local(l) => Some(l.world.timeouts.load(Ordering::SeqCst)),
            _ => None,
        }
    }

    /// Register a handler run when the last handle of this rank's
    /// communicator is dropped, before its MPI handle is freed (upstream
    /// `CommunicatorEventHandler::onCommunicatorFinalize`, MPI-023; e.g.
    /// destroying solver instances bound to the communicator).
    pub fn on_finalize(&self, handler: impl FnOnce() + Send + 'static) {
        self.shared
            .finalize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Box::new(handler));
    }

    /// Local admission of a [`Communicator::native_region`] (no
    /// communication): the thread policy and, under `Serialized`, the
    /// split-class admission of the process (the registration is held by
    /// the ticket). The caller **must agree** [`crate::NativeTicket::verdict`] on
    /// every rank (e.g. as a step of its collective scope) before entering
    /// the region, and drop the ticket (releasing the registration) if the
    /// agreement failed. Non-MPI communicators: a ticket whose verdict is
    /// [`Error::Unsupported`] (identical on every rank).
    #[cfg(feature = "mpi")]
    #[must_use]
    pub fn native_ticket(&self, what: &str) -> crate::mpi_backend::NativeTicket {
        let id = self.identity();
        match self.backend() {
            Backend::Mpi(m) => m.native_ticket(id, what),
            _ => crate::mpi_backend::NativeTicket::refused(id, self.no_native_handle(what)),
        }
    }

    /// Run a native library region `f` with callbacks into akantu — PETSc
    /// `SNESSolve`/`TaoSolve`, whose function/Jacobian callbacks run
    /// collective operations of the caller's scope on the same thread —
    /// after the caller agreed the `ticket`'s verdict on every rank (see
    /// [`Communicator::native_ticket`]).
    ///
    /// `f` runs under the thread policy holding the `Serialized` lock (which
    /// is re-entrant on this thread: the callbacks' own akantu
    /// communications proceed, other threads of the process wait for the
    /// region to end — like the documented blocking calls, their deadlines
    /// are paused), with the ticket's split-class registration (nested
    /// [`Communicator::native_collective`] calls of this thread are covered
    /// by it). Unlike `native_collective`, the region does not check for a
    /// live scope (its callbacks resume the caller's parked scope); nothing
    /// is communicated by the region itself before `f`. Refused locally if
    /// the ticket belongs to another communicator or its verdict is an
    /// error (the caller already agreed the verdict, so this is identical
    /// on every rank), or if the communicator observed a transport failure.
    ///
    /// Thread rules: from the ticket's
    /// admission until the region ends, this thread's nested native
    /// collectives and its native-policy reductions are covered by the
    /// ticket's registration (no second admission), its `mpi_world()` /
    /// imports are refused at once and its splits/duplications are refused
    /// through their headers. Under `Serialized` a long region (a whole
    /// `SNESSolve`) pauses the polled waits of the process's other threads,
    /// refuses their duplications and makes their `mpi_world()` / imports
    /// wait up to their admission deadline: drive native solvers from one
    /// thread and create communicators before or after the regions. The
    /// region thread's emergency abort calls `MPI_Abort` at once (it holds
    /// the lock). The [`crate::NativeComm`] given to `f` cannot be
    /// kept outside it.
    #[cfg(feature = "mpi")]
    pub fn native_region<R>(
        &self,
        ticket: crate::mpi_backend::NativeTicket,
        what: &str,
        f: impl for<'n> FnOnce(crate::mpi_backend::NativeComm<'n>) -> R,
    ) -> Result<R> {
        if let Some(reason) = &*self.scopes.broken() {
            return Err(Error::fatal(
                FatalKind::Transport,
                comm_err(format!(
                    "native region {what} refused: the communicator observed a transport failure ({reason})"
                )),
            ));
        }
        if ticket.issued_for() != self.identity() {
            return Err(Error::invalid(format!(
                "native region {what}: the ticket was issued for another communicator"
            )));
        }
        match self.backend() {
            Backend::Mpi(m) => self.mpi(m.native_region(ticket, what, f)),
            _ => Err(self.no_native_handle(what)),
        }
    }

    #[cfg(feature = "mpi")]
    fn no_native_handle(&self, what: &str) -> Error {
        Error::Unsupported(format!(
            "{what}: native library collectives need an MPI communicator (this one is {})",
            if self.is_serial() {
                "serial"
            } else {
                "an in-process local world"
            }
        ))
    }

    // ---------------------------------------------------------- collectives
    //
    // Every collective builds its descriptor and local validation status
    // (module docs, "Collective contract"); a rank never returns early
    // before its peers learned about an invalid argument.

    /// Barrier.
    pub fn barrier(&self) -> Result<()> {
        self.check_raw("barrier")?;
        let desc = self.desc(CollOp::Barrier);
        match self.backend() {
            Backend::Serial { .. } => Ok(()),
            Backend::Local(l) => l
                .collective(&desc, Ok(()), Vec::new(), |all| {
                    Ok(vec![Vec::new(); all.len()])
                })
                .map(|_| ()),
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => self.mpi(m.barrier(&desc)),
        }
    }

    /// Start a non-blocking barrier (upstream `asyncBarrier`,
    /// `MPI_Ibarrier`). Dropping the request before completion waits for
    /// it (bounded by the backend's deadline); a failure breaks the
    /// communicator's family.
    pub fn ibarrier(&self) -> Result<BarrierRequest> {
        self.check_raw("ibarrier")?;
        let inner = match self.backend() {
            Backend::Serial { .. } => BarrierInner::Done,
            Backend::Local(l) => {
                let seq = l.next_seq();
                let mut st = l.lock()?;
                st.barriers.entry(seq).or_insert((0, 0)).0 += 1;
                l.world.arrived.notify_all();
                BarrierInner::Local {
                    comm: self.clone(),
                    seq,
                }
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => BarrierInner::Mpi {
                comm: self.clone(),
                req: Box::new(self.mpi(m.ibarrier())?),
            },
        };
        Ok(BarrierRequest { inner })
    }

    /// In-place all-reduce. [`ReductionPolicy::RankOrdered`]: folded **in
    /// rank order** (`((v0 ∘ v1) ∘ v2)…`), deterministic for a fixed rank
    /// count and identical in every backend; MPI: one checked all-gather of
    /// the contributions (memory `O(size × len)`: reserved for small,
    /// fixed-size buffers, `docs/ARCHITECTURE.md`). [`ReductionPolicy::NativeMpi`]:
    /// `MPI_Allreduce` with the typed datatype and predefined operator. The
    /// serial result is the identity. Operator, type and length are agreed
    /// on every rank before any data moves.
    pub fn all_reduce<T: CommData>(&self, values: &mut [T], op: ReduceOp) -> Result<()> {
        self.check_raw("all_reduce")?;
        let desc = self
            .desc(CollOp::AllReduce)
            .typed::<T>(Some(values.len()))
            .reducing(op);
        let local = validate_op::<T>(op)
            .and_then(|()| message_bytes(values.len(), T::SIZE, "all_reduce").map(|_| ()));
        match self.backend() {
            Backend::Serial { .. } => local,
            Backend::Local(l) => {
                let bytes = l.collective(&desc, local, to_bytes(values), |all| {
                    let acc = fold_in_rank_order::<T>(&all, op)?;
                    Ok(vec![to_bytes(&acc); all.len()])
                })?;
                from_bytes(&bytes, values)
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => {
                let mine = local.map(|()| to_bytes(values));
                let bytes = match desc.policy {
                    ReductionPolicy::RankOrdered => {
                        let all = self.mpi(m.all_gather_v(&desc, mine))?;
                        to_bytes(&fold_in_rank_order::<T>(&all, op)?)
                    }
                    ReductionPolicy::NativeMpi => self.mpi(m.native_reduce(&desc, mine))?,
                };
                from_bytes(&bytes, values)
            }
        }
    }

    /// Reduction on `root` (upstream `reduce`): `values` holds the result on
    /// the root and is unchanged elsewhere. Rank-ordered policy: gathered on
    /// the root and folded in rank order; native policy: `MPI_Reduce`
    /// (in place on the root). Root, operator, type and length
    /// are agreed on every rank first.
    pub fn reduce<T: CommData>(&self, values: &mut [T], op: ReduceOp, root: Rank) -> Result<()> {
        self.check_raw("reduce")?;
        let desc = self
            .desc(CollOp::Reduce)
            .typed::<T>(Some(values.len()))
            .reducing(op)
            .rooted(root);
        let local = validate_rank(root, self.size(), "reduce")
            .and_then(|()| validate_op::<T>(op))
            .and_then(|()| message_bytes(values.len(), T::SIZE, "reduce").map(|_| ()));
        let is_root = self.rank() == root;
        match self.backend() {
            Backend::Serial { .. } => local,
            Backend::Local(l) => {
                let bytes = l.collective(&desc, local, to_bytes(values), |all| {
                    let acc = fold_in_rank_order::<T>(&all, op)?;
                    let mut out = vec![Vec::new(); all.len()];
                    out[rank_index(root)?] = to_bytes(&acc);
                    Ok(out)
                })?;
                if is_root {
                    from_bytes(&bytes, values)?;
                }
                Ok(())
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => {
                let mine = local.map(|()| to_bytes(values));
                let bytes = match desc.policy {
                    ReductionPolicy::RankOrdered => self
                        .mpi(m.gather_v(&desc, mine, root))?
                        .map(|all| fold_in_rank_order::<T>(&all, op).map(|acc| to_bytes(&acc)))
                        .transpose()?,
                    ReductionPolicy::NativeMpi => {
                        let b = self.mpi(m.native_reduce(&desc, mine))?;
                        is_root.then_some(b)
                    }
                };
                if let Some(bytes) = bytes {
                    from_bytes(&bytes, values)?;
                }
                Ok(())
            }
        }
    }

    /// All-reduce of value/location pairs (`MinLoc`/`MaxLoc`, upstream
    /// `SCMinMaxLoc` with `_min_loc`/`_max_loc`; native policy:
    /// `MPI_DOUBLE_INT` with `MPI_MINLOC`/`MPI_MAXLOC`).
    pub fn all_reduce_loc(&self, values: &mut [ValueLoc], op: ReduceOp) -> Result<()> {
        self.all_reduce(values, op)
    }

    /// Broadcast `values` from `root`. Every rank passes the same length
    /// (agreed with the root and the element type before the payload).
    pub fn broadcast<T: CommData>(&self, values: &mut [T], root: Rank) -> Result<()> {
        self.check_raw("broadcast")?;
        let desc = self
            .desc(CollOp::Broadcast)
            .typed::<T>(Some(values.len()))
            .rooted(root);
        let local = validate_rank(root, self.size(), "broadcast")
            .and_then(|()| message_bytes(values.len(), T::SIZE, "broadcast").map(|_| ()));
        match self.backend() {
            Backend::Serial { .. } => local,
            Backend::Local(l) => {
                let mine = if l.rank == root {
                    to_bytes(values)
                } else {
                    Vec::new()
                };
                let bytes = l.collective(&desc, local, mine, |all| {
                    Ok(vec![all[rank_index(root)?].clone(); all.len()])
                })?;
                from_bytes(&bytes, values)
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => {
                let bytes = self.mpi(m.broadcast(&desc, local.map(|()| to_bytes(values)), root))?;
                from_bytes(&bytes, values)
            }
        }
    }

    /// Broadcast of a dynamically sized buffer (upstream `broadcast` of a
    /// `DynamicCommunicationBuffer`: the size goes first): the root's
    /// `values` are returned on every rank; non-root inputs are ignored.
    pub fn broadcast_vec<T: CommData>(&self, values: &[T], root: Rank) -> Result<Vec<T>> {
        let mut n = [if self.rank() == root {
            u64::try_from(values.len()).unwrap_or(u64::MAX)
        } else {
            0
        }];
        self.broadcast(&mut n, root)?;
        // Every rank got the same length: every rank fails alike.
        let n = usize::try_from(n[0])
            .ok()
            .filter(|&n| message_bytes(n, T::SIZE, "broadcast_vec").is_ok())
            .ok_or_else(|| comm_err("broadcast_vec: length exceeds the message limit"))?;
        let mut out = if self.rank() == root {
            values.to_vec()
        } else {
            vec![T::zero(); n]
        };
        if n > 0 {
            self.broadcast(&mut out, root)?;
        }
        Ok(out)
    }

    /// All-gather fixed-size contributions; returns `size() * values.len()`
    /// items ordered by rank. The length is agreed on every rank before any
    /// data moves (contributions of different lengths fail everywhere; use
    /// [`Communicator::all_gather_v`] for variable sizes).
    pub fn all_gather<T: CommData>(&self, values: &[T]) -> Result<Vec<T>> {
        Ok(self
            .all_gather_impl(values, true)?
            .into_iter()
            .flatten()
            .collect())
    }

    /// All-gather variable-size contributions; returns one vector per rank.
    /// **Memory is `O(Σ sizes)` on every rank**: reserved for small,
    /// bounded payloads (counts, flags, phase descriptors); never for nodal
    /// or global-vector data.
    pub fn all_gather_v<T: CommData>(&self, values: &[T]) -> Result<Vec<Vec<T>>> {
        self.all_gather_impl(values, false)
    }

    fn all_gather_impl<T: CommData>(&self, values: &[T], fixed: bool) -> Result<Vec<Vec<T>>> {
        self.check_raw("all_gather")?;
        let desc = self
            .desc(CollOp::AllGather)
            .typed::<T>(fixed.then_some(values.len()));
        let local = message_bytes(values.len(), T::SIZE, "all_gather").map(|_| ());
        let blocks = match self.backend() {
            Backend::Serial { .. } => return local.map(|()| vec![values.to_vec()]),
            Backend::Local(l) => {
                let bytes = l.collective(&desc, local, to_bytes(values), |all| {
                    let packed = pack_blocks(&all);
                    Ok(vec![packed; all.len()])
                })?;
                unpack_blocks(bytes)?
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => self.mpi(m.all_gather_v(&desc, local.map(|()| to_bytes(values))))?,
        };
        blocks.iter().map(|b| vec_from_bytes(b)).collect()
    }

    /// Gather variable-size contributions on `root` (upstream `gatherV`):
    /// `Some(per-rank vectors)` on the root, `None` elsewhere.
    pub fn gather_v<T: CommData>(&self, values: &[T], root: Rank) -> Result<Option<Vec<Vec<T>>>> {
        self.gather_impl(values, root, false)
    }

    /// Gather fixed-size contributions on `root` (upstream `gather`):
    /// `size() * values.len()` items in rank order on the root. The length
    /// is agreed on every rank first, so unequal contributions fail on
    /// every rank, not only on the root.
    pub fn gather<T: CommData>(&self, values: &[T], root: Rank) -> Result<Option<Vec<T>>> {
        Ok(self
            .gather_impl(values, root, true)?
            .map(|parts| parts.into_iter().flatten().collect()))
    }

    fn gather_impl<T: CommData>(
        &self,
        values: &[T],
        root: Rank,
        fixed: bool,
    ) -> Result<Option<Vec<Vec<T>>>> {
        self.check_raw("gather")?;
        let desc = self
            .desc(CollOp::Gather)
            .typed::<T>(fixed.then_some(values.len()))
            .rooted(root);
        let local = validate_rank(root, self.size(), "gather")
            .and_then(|()| message_bytes(values.len(), T::SIZE, "gather").map(|_| ()));
        let blocks = match self.backend() {
            Backend::Serial { .. } => return local.map(|()| Some(vec![values.to_vec()])),
            Backend::Local(l) => {
                let bytes = l.collective(&desc, local, to_bytes(values), |all| {
                    let mut out = vec![Vec::new(); all.len()];
                    out[rank_index(root)?] = pack_blocks(&all);
                    Ok(out)
                })?;
                if l.rank != root {
                    return Ok(None);
                }
                unpack_blocks(bytes)?
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => {
                match self.mpi(m.gather_v(&desc, local.map(|()| to_bytes(values)), root))? {
                    Some(b) => b,
                    None => return Ok(None),
                }
            }
        };
        blocks
            .iter()
            .map(|b| vec_from_bytes(b))
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }

    /// Scatter variable-size blocks from `root` (`per_rank` is significant
    /// on the root only and must hold one block per rank); returns this
    /// rank's block. Every root-side validation (block count, block sizes,
    /// total size) happens before the agreement, so an invalid root input
    /// fails on every rank before any payload moves.
    pub fn scatter_v<T: CommData>(
        &self,
        per_rank: Option<&[Vec<T>]>,
        root: Rank,
    ) -> Result<Vec<T>> {
        self.check_raw("scatter")?;
        let size = self.size();
        let desc = self.desc(CollOp::Scatter).typed::<T>(None).rooted(root);
        let is_root = self.rank() == root;
        let root_input: Result<Option<Vec<Vec<u8>>>> = validate_rank(root, size, "scatter_v")
            .and_then(|()| match per_rank {
                Some(p) if is_root && p.len() == size => {
                    let mut total = 0usize;
                    for b in p {
                        let n = message_bytes(b.len(), T::SIZE, "scatter_v")?;
                        total = total
                            .checked_add(n)
                            .filter(|&t| t <= MAX_MESSAGE_BYTES)
                            .ok_or_else(|| {
                                Error::InvalidArgument(
                                    "scatter_v: the blocks exceed the message limit in total"
                                        .into(),
                                )
                            })?;
                    }
                    Ok(Some(p.iter().map(|b| to_bytes(b)).collect()))
                }
                Some(p) if is_root => Err(Error::InvalidArgument(format!(
                    "scatter_v: root provided {} blocks for {size} ranks",
                    p.len()
                ))),
                None if is_root => Err(Error::InvalidArgument(
                    "scatter_v: the root must provide the blocks".into(),
                )),
                _ => Ok(None),
            });
        let bytes = match self.backend() {
            Backend::Serial { .. } => root_input?.and_then(|mut v| v.pop()).unwrap_or_default(),
            Backend::Local(l) => {
                let (local, mine) = match root_input {
                    Ok(Some(b)) => (Ok(()), pack_blocks(&b)),
                    Ok(None) => (Ok(()), Vec::new()),
                    Err(e) => (Err(e), Vec::new()),
                };
                l.collective(&desc, local, mine, |all| {
                    let from_root = all.into_iter().nth(rank_index(root)?).unwrap_or_default();
                    unpack_blocks(from_root)
                })?
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => self.mpi(m.scatter_v(&desc, root_input, root))?,
        };
        vec_from_bytes(&bytes)
    }

    /// Scatter equal parts of `values` (root only; length a multiple of
    /// `size()`) from `root`; returns this rank's part.
    pub fn scatter<T: CommData>(&self, values: Option<&[T]>, root: Rank) -> Result<Vec<T>> {
        let size = self.size();
        let blocks: Option<Vec<Vec<T>>> = match values {
            Some(v) if self.rank() == root => {
                if size > 0 && v.len().is_multiple_of(size) {
                    let n = v.len() / size;
                    Some((0..size).map(|r| v[r * n..(r + 1) * n].to_vec()).collect())
                } else {
                    // Invalid on the root: reported on every rank.
                    Some(Vec::new())
                }
            }
            _ => None,
        };
        self.scatter_v(blocks.as_deref(), root)
    }

    fn scan_impl<T: CommData>(
        &self,
        values: &mut [T],
        op: ReduceOp,
        exclusive: bool,
    ) -> Result<()> {
        self.check_raw("scan")?;
        let code = if exclusive {
            CollOp::ExclusiveScan
        } else {
            CollOp::Scan
        };
        let desc = self.desc(code).typed::<T>(Some(values.len())).reducing(op);
        let local = validate_op::<T>(op)
            .and_then(|()| message_bytes(values.len(), T::SIZE, "scan").map(|_| ()));
        match self.backend() {
            Backend::Serial { .. } => {
                local?;
                if exclusive {
                    values.fill(T::zero());
                }
                Ok(())
            }
            Backend::Local(l) => {
                let bytes = l.collective(&desc, local, to_bytes(values), |all| {
                    Ok(prefix_folds::<T>(&all, op, exclusive)?
                        .iter()
                        .map(|v| to_bytes(v))
                        .collect())
                })?;
                from_bytes(&bytes, values)
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => {
                let mine = local.map(|()| to_bytes(values));
                let bytes = match desc.policy {
                    ReductionPolicy::RankOrdered => {
                        let all = self.mpi(m.all_gather_v(&desc, mine))?;
                        let folds = prefix_folds::<T>(&all, op, exclusive)?;
                        to_bytes(
                            folds
                                .get(rank_index(m.rank())?)
                                .ok_or_else(|| comm_err("scan: missing own result"))?,
                        )
                    }
                    ReductionPolicy::NativeMpi => self.mpi(m.native_reduce(&desc, mine))?,
                };
                from_bytes(&bytes, values)
            }
        }
    }

    /// Inclusive prefix reduction over ranks (upstream `scan`): rank order,
    /// or `MPI_Scan` under the native policy.
    pub fn scan<T: CommData>(&self, values: &mut [T], op: ReduceOp) -> Result<()> {
        self.scan_impl(values, op, false)
    }

    /// Exclusive prefix reduction (upstream `exclusiveScan`): rank 0 gets
    /// zero (every element; MPI leaves rank 0's `MPI_Exscan` result
    /// undefined and upstream only resets the first element). Native
    /// policy: `MPI_Exscan`.
    pub fn exclusive_scan<T: CommData>(&self, values: &mut [T], op: ReduceOp) -> Result<()> {
        self.scan_impl(values, op, true)
    }

    /// Exclusive prefix sum of one value.
    pub fn exclusive_scan_sum_u64(&self, value: u64) -> Result<u64> {
        let mut v = [value];
        self.exclusive_scan(&mut v, ReduceOp::Sum)?;
        Ok(v[0])
    }

    // ------------------------------------------------- derived communicators

    /// Duplicate this communicator (collective; MPI: `MPI_Comm_idup`,
    /// polled under the progress deadline): an
    /// independent message space with its own scope registry (sharing this
    /// one's transport-failure state) and the same options, reduction
    /// policy included.
    ///
    /// **Communicator creation under `MPI_THREAD_SERIALIZED`**
    /// (`docs/ARCHITECTURE.md`): every creation (split,
    /// duplication, world adoption, import) and every native-policy
    /// reduction takes part in one per-process admission rule
    /// (`mpi_backend::ExclusiveGuard`) that keeps Open MPI's blocking
    /// `MPI_Comm_split` from deadlocking across ranks with another thread's
    /// creation. Refusals are agreed, non-fatal `Error::Communication`s and
    /// the communicator stays usable, but there is **no fairness**: under
    /// contention from several threads a creation can be refused
    /// repeatedly — serialise creations per process, or retry with
    /// randomised back-off. Under `Multiple`, `Single` and `Funneled` the
    /// rule does not apply.
    /// A duplication is refused (agreed by its header, on every rank) while
    /// a committed split is in flight in the process, and stays registered
    /// until its `MPI_Comm_idup` completed — for ever if it was leaked after
    /// a timeout, which then refuses every later split of the process and
    /// delays later duplications behind its context id (the process must
    /// abort).
    pub fn duplicate(&self) -> Result<Communicator> {
        self.check_raw("duplicate")?;
        match self.backend() {
            Backend::Serial { .. } => Ok(Self::from_backend(
                Backend::Serial {
                    state: Mutex::new(SerialState::default()),
                },
                self.scopes.derived(),
            )),
            Backend::Local(l) => {
                let c = self
                    .local_split(l, CollOp::Duplicate, Ok(Some(0)), self.rank())?
                    .ok_or_else(|| comm_err("duplicate: missing communicator"))?;
                Ok(c)
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => {
                let b = self.mpi(m.duplicate(&self.desc(CollOp::Duplicate)))?;
                Ok(Self::from_backend(Backend::Mpi(b), self.scopes.derived()))
            }
        }
    }

    /// Split this communicator by `color` (collective, `MPI_Comm_split`;
    /// ranks ordered by `(key, rank)`); `None` (`MPI_UNDEFINED`) yields no
    /// communicator. A color above `i32::MAX` is invalid (on every
    /// backend; agreed on every rank). Derived communicators have their own
    /// scope registry, share this one's transport-failure state and inherit
    /// its options (reduction policy included).
    ///
    /// MPI: `MPI_Comm_split` is blocking (MPI-3 has no non-blocking form):
    /// after its checked headers it has **no progress deadline**. Under
    /// `MPI_THREAD_SERIALIZED` it runs a validation header, then
    /// **commits** — refused while a [`ReductionPolicy::NativeMpi`]
    /// reduction/scan, another committed split or **any** communicator
    /// duplication (`duplicate`, `mpi_world`, imports) is in flight in the
    /// process — agreed by a second (commit) header, so a refusal is the
    /// same non-fatal `Error::Communication` on every rank before any
    /// communicator is created
    /// (`docs/ARCHITECTURE.md`).
    ///
    /// **Communicator creation under `MPI_THREAD_SERIALIZED`**
    /// (`docs/ARCHITECTURE.md`): every creation (split,
    /// duplication, world adoption, import) and every native-policy
    /// reduction takes part in one per-process admission rule
    /// (`mpi_backend::ExclusiveGuard`) that keeps Open MPI's blocking
    /// `MPI_Comm_split` from deadlocking across ranks with another thread's
    /// creation. Refusals are agreed, non-fatal `Error::Communication`s and
    /// the communicator stays usable, but there is **no fairness**: under
    /// contention from several threads a creation can be refused
    /// repeatedly — serialise creations per process, or retry with
    /// randomised back-off. Under `Multiple`, `Single` and `Funneled` the
    /// rule does not apply.
    pub fn split(&self, color: Option<u32>, key: i32) -> Result<Option<Communicator>> {
        self.check_raw("split")?;
        let checked: Result<Option<u32>> = match color {
            Some(c) if i32::try_from(c).is_err() => Err(Error::InvalidArgument(format!(
                "split: color {c} exceeds the MPI int range"
            ))),
            c => Ok(c),
        };
        match self.backend() {
            Backend::Serial { .. } => Ok(checked?.map(|_| {
                Self::from_backend(
                    Backend::Serial {
                        state: Mutex::new(SerialState::default()),
                    },
                    self.scopes.derived(),
                )
            })),
            Backend::Local(l) => self.local_split(l, CollOp::Split, checked, key),
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => {
                let b = self.mpi(m.split(&self.desc(CollOp::Split), checked, key))?;
                Ok(b.map(|b| Self::from_backend(Backend::Mpi(b), self.scopes.derived())))
            }
        }
    }

    /// Run `f` — one or several **blocking collective calls of a native
    /// library** (MUMPS `dmumps_c`, PETSc) — on this communicator's MPI
    /// handle, on every rank (`docs/ARCHITECTURE.md`; one interface for
    /// MUMPS and PETSc).
    ///
    /// Protocol: `local` (the caller's local validation), the job identity
    /// `what` and, under `MPI_THREAD_SERIALIZED`, the admission verdict are
    /// agreed by one checked header before anything is handed to the
    /// library, so a local failure, a refusal or ranks entering different
    /// native jobs fail **every** rank with an ordinary
    /// error and nobody enters the library alone. `what` must therefore be
    /// identical on every rank (derived from object or solver ids, never
    /// from rank-local data). The MPI handles handed to `f` are converted
    /// inside the same thread-policy call as `f` (the Fortran conversion is
    /// an MPI call). Native libraries create
    /// communicators inside their calls (MUMPS duplicates its communicator
    /// at `JOB = -1`, builds BLACS grids, PT-Scotch splits; PETSc
    /// duplicates every new communicator it sees), i.e. they allocate
    /// context ids in blocking calls; under `Serialized` the call is
    /// therefore admitted like a committed `MPI_Comm_split` (refused while
    /// a native reduction, a committed split or a communicator duplication
    /// is in flight in this process) and `f` runs
    /// holding the process MPI lock. Under `MPI_THREAD_MULTIPLE` the same
    /// registration refuses a native collective while another one is in
    /// flight in the process (agreed, ordinary error; native libraries make
    /// no promise for concurrent instances on several threads).
    /// Inside a [`Communicator::native_region`] (or another native
    /// collective) of the same thread the enclosing registration covers the
    /// call: no second admission. The thread policy is part of
    /// the local validation. Like the native reductions, `f` has **no
    /// progress deadline**: a peer that stalls inside
    /// the library stalls every rank. `f` must issue the same native
    /// collectives on every rank (library contract) and must not call back
    /// into akantu communication (use [`Communicator::native_region`] for
    /// libraries with callbacks).
    ///
    /// A raw operation: refused while a collective scope is live on this
    /// communicator, except inside one of that scope's communication steps
    /// (`Phases::run` closures). The serial backend and the local world
    /// have no MPI handle: [`Error::Unsupported`] on every rank (the outcome
    /// is identical everywhere, nothing is communicated).
    ///
    /// Inside a communication step, pass the result through
    /// [`crate::phase::agreed_or_transport`]: every error other than a
    /// transport failure is agreed by the header and must stay an ordinary
    /// (inner) error of the step.
    ///
    /// The [`crate::NativeComm`] capability is bound to the call:
    /// it cannot be kept outside `f`.
    ///
    /// ```no_run
    /// # fn f(c: &akantu_comm::Communicator) -> akantu_core::Result<()> {
    /// let mut size = None;
    /// c.native_collective("example", Ok(()), |nc| size = Some(nc.size()))?;
    /// # Ok(()) }
    /// ```
    ///
    /// ```compile_fail
    /// # fn f(c: &akantu_comm::Communicator) -> akantu_core::Result<()> {
    /// let mut kept = None;
    /// c.native_collective("escape", Ok(()), |nc| kept = Some(nc))?;
    /// let _later = kept; // the capability outlives the agreed entry
    /// # Ok(()) }
    /// ```
    #[cfg(feature = "mpi")]
    pub fn native_collective<R>(
        &self,
        what: &str,
        local: Result<()>,
        f: impl for<'n> FnOnce(crate::mpi_backend::NativeComm<'n>) -> R,
    ) -> Result<R> {
        self.check_raw(what)?;
        match self.backend() {
            Backend::Mpi(m) => self.mpi(m.native_collective(
                &self.desc(CollOp::Native),
                what,
                local,
                self.identity(),
                f,
            )),
            _ => Err(self.no_native_handle(what)),
        }
    }

    /// Queue the destruction of a native library object whose destruction
    /// is itself a blocking collective (MUMPS `JOB = -2`, PETSc `*Destroy`,
    /// `PetscFinalize`) through the MPI lifecycle
    /// (`Lifecycle::release_handle`, `docs/ARCHITECTURE.md`): it runs at once if
    /// the current thread may call MPI (under the `Serialized` lock),
    /// otherwise on MPI's main thread — always before `MPI_Finalize`. No
    /// header can be agreed in a destructor: every rank must release its
    /// object at the same point of the program (SPMD order, as upstream's
    /// destructors), and under `Serialized` not while another thread of the
    /// process is in a blocking collective — in practice: drop native
    /// objects on MPI's main thread, at the same program point on every
    /// rank, with no other thread inside MPI (under
    /// `Funneled` a drop on a worker is deferred to the main thread's next
    /// lifecycle drain, which moves the collective destruction point).
    /// Never queue it for a communicator that is [`Communicator::broken`]:
    /// abandon the object instead. Returns `false` (and
    /// drops `destroy` unrun) for a non-MPI communicator or when MPI is not
    /// active (nothing to free).
    #[cfg(feature = "mpi")]
    pub fn release_native(&self, destroy: impl FnOnce() + Send + 'static) -> bool {
        match self.backend() {
            Backend::Mpi(_) => {
                crate::mpi_backend::mpi_lifecycle().release_handle(destroy);
                true
            }
            _ => false,
        }
    }

    fn local_split(
        &self,
        l: &LocalRank,
        op: CollOp,
        color: Result<Option<u32>>,
        key: i32,
    ) -> Result<Option<Communicator>> {
        let (local, color) = match color {
            Ok(c) => (Ok(()), c),
            Err(e) => (Err(e), None),
        };
        let mut payload = CommunicationBuffer::new();
        payload.pack(&u8::from(color.is_some()));
        payload.pack(&color.unwrap_or(0));
        payload.pack(&key);
        // The collective's sequence number identifies the child worlds.
        let seq = l.collective_seq.load(Ordering::SeqCst);
        let desc = self.desc(op);
        let mine = l.collective(&desc, local, payload.into_bytes(), |all| {
            let mut groups: std::collections::BTreeMap<u32, Vec<(i32, usize)>> =
                std::collections::BTreeMap::new();
            for (r, bytes) in all.iter().enumerate() {
                let mut b = CommunicationBuffer::from_bytes(bytes.clone());
                let has = b.unpack::<u8>()? != 0;
                let c = b.unpack::<u32>()?;
                let k = b.unpack::<i32>()?;
                if has {
                    groups.entry(c).or_default().push((k, r));
                }
            }
            let mut out = vec![Vec::new(); all.len()];
            let mut st = l.lock()?;
            for (c, mut members) in groups {
                members.sort_unstable();
                let world = Arc::new(LocalWorld {
                    size: members.len(),
                    state: Mutex::new(WorldState::default()),
                    arrived: Condvar::new(),
                    timeout: l.world.timeout,
                    mode: l.world.mode,
                    timeouts: Arc::clone(&l.world.timeouts),
                });
                st.children.insert((seq, c), (world, members.len()));
                for (new_rank, (_, r)) in members.into_iter().enumerate() {
                    let mut m = CommunicationBuffer::new();
                    m.pack(&u64::try_from(new_rank).unwrap_or(u64::MAX));
                    out[r] = m.into_bytes();
                }
            }
            Ok(out)
        })?;
        let Some(c) = color else {
            return Ok(None);
        };
        let new_rank = CommunicationBuffer::from_bytes(mine).unpack::<u64>()?;
        let world = {
            let mut st = l.lock()?;
            let entry = st
                .children
                .get_mut(&(seq, c))
                .ok_or_else(|| comm_err("split: child world missing"))?;
            let w = Arc::clone(&entry.0);
            entry.1 -= 1;
            if entry.1 == 0 {
                st.children.remove(&(seq, c));
            }
            w
        };
        Ok(Some(Self::from_backend(
            Backend::Local(LocalRank {
                rank: Rank::try_from(new_rank).map_err(|_| comm_err("rank overflow"))?,
                world,
                collective_seq: AtomicU64::new(0),
            }),
            self.scopes.derived(),
        )))
    }

    // ------------------------------------------------------- point to point

    /// Blocking send. Completes when the buffer may be reused: at once for
    /// the serial backend and eager local worlds, after the matching
    /// receive for [`SendMode::Rendezvous`] (MPI: `MPI_Isend` + wait, which
    /// may block until the receive is matched — never rely on buffering).
    pub fn send<T: CommData>(&self, values: &[T], dest: Rank, tag: Tag) -> Result<()> {
        self.send_mode(values, dest, tag, SendMode::Eager)
    }

    /// Blocking send with an explicit mode (upstream `CommunicationMode`;
    /// `Rendezvous` = `_synchronous`).
    pub fn send_mode<T: CommData>(
        &self,
        values: &[T],
        dest: Rank,
        tag: Tag,
        mode: SendMode,
    ) -> Result<()> {
        self.check_raw("send")?;
        self.isend_unchecked(values.to_vec(), dest, tag, mode)?
            .wait()
            .map(|_| ())
    }

    /// Non-blocking send of an **owned** buffer: the request keeps the
    /// buffer until completion and returns it from [`SendRequest::wait`].
    pub fn isend<T: CommData>(
        &self,
        values: Vec<T>,
        dest: Rank,
        tag: Tag,
    ) -> Result<SendRequest<T>> {
        self.isend_mode(values, dest, tag, SendMode::Eager)
    }

    /// Non-blocking send with an explicit mode (`Rendezvous`: completes
    /// only once matched — required by [`Communicator::receive_any_number`]).
    pub fn isend_mode<T: CommData>(
        &self,
        values: Vec<T>,
        dest: Rank,
        tag: Tag,
        mode: SendMode,
    ) -> Result<SendRequest<T>> {
        self.check_raw("isend")?;
        self.isend_unchecked(values, dest, tag, mode)
    }

    fn isend_unchecked<T: CommData>(
        &self,
        values: Vec<T>,
        dest: Rank,
        tag: Tag,
        mode: SendMode,
    ) -> Result<SendRequest<T>> {
        self.check_rank(dest)?;
        message_bytes(values.len(), T::SIZE, "send")?;
        // the buffer `SendRequest::wait` returns (a byte buffer sent by MPI
        // is moved to the backend and comes back from it instead)
        #[allow(unused_mut)] // emptied on the MPI path only
        let mut kept: Option<Vec<T>> = Some(values);
        let values: &[T] = kept.as_deref().unwrap_or(&[]);
        let pending = match self.backend() {
            Backend::Serial { state } => {
                let mut st = state.lock().map_err(|_| comm_err("poisoned mailbox"))?;
                if let Err(bytes) = st.posted.deliver(tag, to_bytes(values)) {
                    st.queue.entry(tag).or_default().push_back(bytes);
                }
                None
            }
            Backend::Local(l) => {
                let sync = mode == SendMode::Rendezvous || l.world.mode == SendMode::Rendezvous;
                let id = l.push(dest, Channel::P2P(tag), to_bytes(values), sync)?;
                sync.then(|| PendingSend::Local {
                    comm: self.clone(),
                    id,
                    dest,
                })
            }
            // Messages to self never enter MPI.
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) if dest == m.rank() => self
                .mpi(m.send_to_self(to_bytes(values), tag.0, mode == SendMode::Rendezvous))?
                .map(|id| PendingSend::MpiSelf {
                    comm: self.clone(),
                    id,
                }),
            // A byte buffer is handed to MPI as it is (no copy) and given
            // back by `SendRequest::wait`; other types are packed.
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => {
                let bytes = match byte_vec(kept.take().unwrap_or_default()) {
                    Ok(bytes) => bytes,
                    Err(v) => {
                        let b = to_bytes(&v);
                        kept = Some(v);
                        b
                    }
                };
                Some(PendingSend::Mpi {
                    comm: self.clone(),
                    req: Box::new(self.mpi(m.isend(
                        bytes,
                        dest,
                        tag.0,
                        mode == SendMode::Rendezvous,
                    ))?),
                })
            }
        };
        Ok(SendRequest {
            completed: pending.is_none(),
            buffer: kept,
            pending,
        })
    }

    /// Blocking receive of exactly `values.len()` items (matched like
    /// [`Communicator::receive_vec`]; a message of another length is an
    /// error and is consumed).
    pub fn receive<T: CommData>(&self, values: &mut [T], source: Rank, tag: Tag) -> Result<()> {
        self.check_raw("receive")?;
        let bytes = self.receive_bytes(source, tag)?;
        from_bytes(&bytes, values)
    }

    /// Blocking receive of the next message from `source` with `tag` not
    /// matched by a posted receive, whatever its length (upstream receive
    /// into a dynamic buffer, which probes the size first). MPI: matching
    /// probe, then `MPI_Imrecv` of exactly the matched length, both under
    /// one operation deadline.
    pub fn receive_vec<T: CommData>(&self, source: Rank, tag: Tag) -> Result<Vec<T>> {
        self.check_raw("receive")?;
        vec_from_bytes(&self.receive_bytes(source, tag)?)
    }

    /// Matched receive of the next unmatched message (no admission check).
    fn receive_bytes(&self, source: Rank, tag: Tag) -> Result<Vec<u8>> {
        self.check_rank(source)?;
        match self.backend() {
            Backend::Serial { state } => state
                .lock()
                .map_err(|_| comm_err("poisoned mailbox"))?
                .queue
                .get_mut(&tag)
                .and_then(VecDeque::pop_front)
                .ok_or_else(|| {
                    comm_err(format!(
                        "serial receive from {source} with tag {tag:?}: no matching send (would dead-lock)"
                    ))
                }),
            Backend::Local(l) => l.pop(source, Channel::P2P(tag), &format!("message {tag:?}")),
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => self.mpi(m.recv(source, tag.0)),
        }
    }

    /// Post a non-blocking receive of up to `len` items from `source` with
    /// `tag` (upstream `asyncReceive`): messages from one source with one
    /// tag match posted receives in **posting order**, and a posted receive
    /// lets the sender's synchronous send complete before the receive is
    /// waited on (MPI: while this process waits in any
    /// akantu operation — every akantu wait runs the matching engine). The
    /// request hands the message out on completion ([`RecvRequest::wait`]:
    /// exactly `len` items; [`RecvRequest::wait_received`]: the actual
    /// length, at most `len`). A longer message is consumed and reported as
    /// truncated — an ordinary error on completion, never a transport
    /// failure, and never written past a buffer (MPI receives always have
    /// the matched message's exact length). MPI: the
    /// posted length does **not** bound the memory of a longer message — it
    /// is received whole (one allocation of its length, at most
    /// [`MAX_MESSAGE_BYTES`]) before the truncation is reported
    /// (Open MPI 5.0.7 cannot truncate safely, and MPI has no
    /// partial receive of a matched message). Dropping an incomplete request
    /// cancels it (see the module docs).
    pub fn irecv<T: CommData>(&self, len: usize, source: Rank, tag: Tag) -> Result<RecvRequest<T>> {
        self.check_raw("irecv")?;
        self.check_rank(source)?;
        let bytes = message_bytes(len, T::SIZE, "irecv")?;
        let pending = match self.backend() {
            Backend::Serial { state } => {
                let mut st = state.lock().map_err(|_| comm_err("poisoned mailbox"))?;
                let matched = st.queue.get_mut(&tag).and_then(VecDeque::pop_front);
                PendingRecv::Serial {
                    slot: st.posted.post(tag, matched),
                }
            }
            Backend::Local(l) => PendingRecv::Local {
                slot: l.post_recv(source, tag)?,
            },
            // Receives from self never enter MPI.
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) if source == m.rank() => PendingRecv::MpiSelf {
                slot: self.mpi(m.post_from_self(bytes, tag.0))?,
            },
            // A slot of the MPI backend's matching engine: MPI never gets a
            // receive buffer shorter than the matched message
            // (Open MPI 5.0.7 overruns truncated receives).
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => PendingRecv::Mpi {
                slot: self.mpi(m.post_recv(source, tag.0))?,
            },
        };
        Ok(RecvRequest {
            comm: self.clone(),
            source,
            tag,
            len,
            bytes,
            pending: Some(pending),
            _item: std::marker::PhantomData,
        })
    }

    fn probe_impl<T: CommData>(
        &self,
        source: Option<Rank>,
        tag: Tag,
        block: bool,
    ) -> Result<Option<Status>> {
        if let Some(s) = source {
            self.check_rank(s)?;
        }
        let found: Option<(Rank, usize)> = match self.backend() {
            Backend::Serial { state } => {
                let st = state.lock().map_err(|_| comm_err("poisoned mailbox"))?;
                let hit = st
                    .queue
                    .get(&tag)
                    .and_then(VecDeque::front)
                    .map(|m| (0, m.len()));
                if hit.is_none() && block {
                    return Err(comm_err(format!(
                        "serial probe with tag {tag:?}: no matching send (would dead-lock)"
                    )));
                }
                hit
            }
            Backend::Local(l) => {
                if block {
                    Some(l.wait_for(&|| format!("a message with tag {tag:?}"), |st| {
                        l.peek(st, source, tag)
                    })?)
                } else {
                    l.peek(&*l.lock()?, source, tag)
                }
            }
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => self
                .mpi(m.probe(source, tag.0, block))?
                .map(|(s, _, n)| (s, n)),
        };
        found
            .map(|(src, bytes)| {
                if bytes.is_multiple_of(T::SIZE) {
                    Ok(Status {
                        source: src,
                        tag,
                        count: bytes / T::SIZE,
                    })
                } else {
                    Err(comm_err(format!(
                        "probed message of {bytes} bytes is not a whole number of items"
                    )))
                }
            })
            .transpose()
    }

    /// Blocking probe (upstream `probe<T>`; `source = None`:
    /// `MPI_ANY_SOURCE`): the status of the next matching message, count in
    /// items of `T`.
    pub fn probe<T: CommData>(&self, source: Option<Rank>, tag: Tag) -> Result<Status> {
        self.check_raw("probe")?;
        self.probe_impl::<T>(source, tag, true)?
            .ok_or_else(|| comm_err("probe: no message"))
    }

    /// Non-blocking probe (upstream `asyncProbe<T>`).
    pub fn iprobe<T: CommData>(&self, source: Option<Rank>, tag: Tag) -> Result<Option<Status>> {
        self.check_raw("iprobe")?;
        self.probe_impl::<T>(source, tag, false)
    }

    /// Dynamic sparse data exchange with an unknown number of incoming
    /// messages (upstream `receiveAnyNumber`, MPI-022; NBX: probe any source
    /// → receive → `process(source, message)`; once every send completed,
    /// enter a non-blocking barrier; done when it completes).
    ///
    /// Contract: `sends` must be **synchronous** (`isend_mode(…,
    /// SendMode::Rendezvous)`), so their completion proves the message was
    /// matched; every rank calls it collectively; and `tag` must be unique
    /// to this exchange among exchanges that can overlap — a rank leaving
    /// the barrier may start the next exchange while a peer still probes, so
    /// reusing one tag in consecutive rounds lets a peer receive a message
    /// of the next round (the probable cause of the upstream
    /// `Communicator.ReceiveAny` failures, UD-076). A failing `process` does
    /// not abort the protocol (its peers would hang): the remaining messages
    /// are drained and the first error is returned. Bounded by the backend's
    /// progress deadline.
    pub fn receive_any_number<T: CommData>(
        &self,
        sends: Vec<SendRequest<T>>,
        tag: Tag,
        mut process: impl FnMut(Rank, Vec<T>) -> Result<()>,
    ) -> Result<()> {
        if self.size() == 1 {
            for s in sends {
                s.wait()?;
            }
            return Ok(());
        }
        let deadline = self.progress_timeout().map(|t| Instant::now() + t);
        let mut sends = sends;
        let mut barrier: Option<BarrierRequest> = None;
        let mut first_err: Option<Error> = None;
        let mut spins = 0u32;
        loop {
            let mut progressed = false;
            while let Some(st) = self.iprobe::<T>(None, tag)? {
                let msg = self.receive_vec::<T>(st.source, tag)?;
                if let Err(e) = process(st.source, msg)
                    && first_err.is_none()
                {
                    first_err = Some(e);
                }
                progressed = true;
            }
            match &mut barrier {
                None => {
                    let mut all = true;
                    for s in &mut sends {
                        all &= s.test()?;
                    }
                    if all {
                        barrier = Some(self.ibarrier()?);
                        progressed = true;
                    }
                }
                Some(b) => {
                    if b.test()? {
                        break;
                    }
                }
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                let e = comm_err(format!(
                    "rank {}: receive_any_number (tag {tag:?}) exceeded the progress deadline",
                    self.rank()
                ));
                if self.is_mpi() {
                    let e = Error::fatal(FatalKind::Transport, e);
                    self.mark_broken(&e.to_string());
                    return Err(e);
                }
                // the local world's timeout ended this wait (counted)
                if let Backend::Local(l) = self.backend() {
                    l.world.timed_out();
                }
                return Err(e);
            }
            if progressed {
                spins = 0;
            } else {
                spins += 1;
                if spins < 256 {
                    std::thread::yield_now();
                } else {
                    std::thread::sleep(Duration::from_micros(50));
                }
            }
        }
        for s in sends {
            s.wait()?;
        }
        first_err.map_or(Ok(()), Err)
    }

    /// Abort the job (upstream `MPI_Abort` through the error handler; the
    /// top-level runtime's transport-failure path); never returns. Serial
    /// and local world: exit the process with `code`. MPI:
    /// `MPI_Abort(comm, code)` only where the provided thread level
    /// permits it on the calling thread — any thread under `Multiple`; under
    /// `Serialized` only after acquiring the process-wide MPI lock within a
    /// bounded wait (a thread stuck in an MPI call while holding it must not
    /// block the abort forever); MPI's main thread under `Single`/`Funneled`.
    /// Otherwise the **non-MPI fallback** asks the launcher to abort the
    /// job with `code` through `PMIx_Abort` when the process has a PMIx
    /// client (Open MPI 5; the PMIx half of Open MPI's own `MPI_Abort`, not
    /// an MPI call; waited for at most 2 s) and then terminates this
    /// process with `code` (`_exit`: no destructors, no `atexit` handlers,
    /// no MPI call); without PMIx the MPI launcher tears down the job when
    /// it sees the non-zero exit status without finalisation. Safe to call
    /// from any thread.
    pub fn abort(&self, code: i32) -> ! {
        match self.backend() {
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => m.abort(code),
            _ => std::process::exit(code),
        }
    }

    /// Test hook (MPI only): issue an MPI call with an invalid source rank
    /// so MPI itself returns an error code (`MPI_ERR_RANK`) through the
    /// communicator's `MPI_ERRORS_RETURN` handler; the result is the
    /// translated `Error::Fatal { kind: Transport }` and the communicator is
    /// broken. Serial and local-world communicators return `Ok(())`.
    #[doc(hidden)]
    pub fn debug_inject_mpi_error(&self) -> Result<()> {
        match self.backend() {
            #[cfg(feature = "mpi")]
            Backend::Mpi(m) => self.mpi(m.inject_invalid_rank_error()),
            _ => Ok(()),
        }
    }

    /// The runtime abort policy (`docs/ARCHITECTURE.md`): an
    /// `Error::Fatal { kind: Transport }` means peers may be blocked in a
    /// collective — abort the job with [`TRANSPORT_ABORT_CODE`]; any other
    /// error is returned to the caller unchanged (agreed fatal errors allow
    /// an orderly shutdown).
    ///
    /// On the MPI backend the abort path performs **no symbol lookup, no
    /// thread creation and no allocation from this function on**:
    /// the note is assembled from static text and integers
    /// (rank, the numeric [`Error::code`] of the failure, the exit code) in
    /// a stack buffer, the thread check uses the thread key (never
    /// `std::thread::current()`), and the backend's emergency abort follows
    /// (`MPI_Abort`, or the bounded `PMIx_Abort` fallback and `_exit`). The
    /// error's text is therefore not printed here: whoever detected the
    /// failure reports it (the caller still holds the error).
    pub fn abort_on_transport_fatal(&self, error: &Error) {
        if error.fatal_kind() == Some(FatalKind::Transport) {
            emergency_note(transport_abort_note(self.rank(), error).as_str());
            self.abort(TRANSPORT_ABORT_CODE);
        }
    }

    fn check_rank(&self, r: Rank) -> Result<()> {
        match usize::try_from(r) {
            Ok(u) if u < self.size() => Ok(()),
            _ => Err(comm_err(format!(
                "rank {r} out of range (size {})",
                self.size()
            ))),
        }
    }
}

/// `code` in decimal, into `buf` (no allocation; the notes of the abort
/// paths).
pub(crate) fn decimal(code: i32, buf: &mut [u8; 11]) -> &[u8] {
    let mut n = i64::from(code).unsigned_abs();
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + u8::try_from(n % 10).unwrap_or(0);
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if code < 0 {
        i -= 1;
        buf[i] = b'-';
    }
    &buf[i..]
}

/// Capacity of a [`StackNote`] (the longest note of
/// [`Communicator::abort_on_transport_fatal`] is ≈ 100 bytes).
const STACK_NOTE_CAPACITY: usize = 192;

/// The note of [`Communicator::abort_on_transport_fatal`] (no allocation):
/// the rank, the numeric [`Error::code`] of the failure (of the wrapped
/// error for an `Error::Fatal`) and the exit code.
fn transport_abort_note(rank: Rank, error: &Error) -> StackNote {
    let code = match error {
        Error::Fatal { source, .. } => source.code(),
        other => other.code(),
    };
    let mut note = StackNote::new();
    note.text("akantu: rank ")
        .int(rank)
        .text(": transport failure (akantu error code ")
        .int(code)
        .text("), aborting the job with exit code ")
        .int(TRANSPORT_ABORT_CODE)
        .text("\n");
    note
}

/// An ASCII diagnostic assembled in a fixed stack buffer:
/// static text and integers only, so the abort entry point writes
/// it without allocating, in a single write. Text beyond the capacity is
/// cut (never a panic).
struct StackNote {
    buf: [u8; STACK_NOTE_CAPACITY],
    len: usize,
}

impl StackNote {
    fn new() -> Self {
        Self {
            buf: [0; STACK_NOTE_CAPACITY],
            len: 0,
        }
    }

    /// Append ASCII `text` (cut at the capacity).
    fn text(&mut self, text: &'static str) -> &mut Self {
        debug_assert!(text.is_ascii(), "StackNote holds ASCII only");
        self.bytes(text.as_bytes())
    }

    /// Append `value` in decimal.
    fn int(&mut self, value: i32) -> &mut Self {
        let mut digits = [0u8; 11];
        self.bytes(decimal(value, &mut digits))
    }

    fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        let room = STACK_NOTE_CAPACITY - self.len;
        let n = bytes.len().min(room);
        self.buf[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
        self
    }

    /// The note (ASCII by construction; empty if it were not).
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

/// Diagnostic of the abort paths on stderr that never panics
/// (`eprintln!` panics when stderr fails, e.g. `EPIPE`); errors are
/// ignored. MPI builds write with POSIX `write` (no `Stderr` lock).
fn emergency_note(msg: &str) {
    #[cfg(feature = "mpi")]
    crate::mpi_backend::emergency_note(msg);
    #[cfg(not(feature = "mpi"))]
    {
        use std::io::Write as _;
        let _ = std::io::stderr().write_all(msg.as_bytes());
    }
}

/// Run `f` on every rank of a fresh in-process world (one scoped thread per
/// rank, eager sends) and return the per-rank results in rank order. A
/// panic on any rank is propagated. Each rank thread runs under its own
/// debugger scope ([`akantu_core::debug::rank_scope`]: a copy of the
/// calling thread's debugger with the rank's parallel context), as each
/// upstream rank is a process with its own debugger.
pub fn run_local<T, F>(size: usize, timeout: Duration, f: F) -> Vec<T>
where
    T: Send,
    F: Fn(Communicator) -> T + Sync,
{
    run_local_with_mode(size, timeout, SendMode::Eager, f)
}

/// Stack size of a local-world rank thread (the default main-thread stack
/// of a Linux process, where a real MPI rank runs).
pub const RANK_THREAD_STACK: usize = 8 * 1024 * 1024;

/// [`run_local`] with explicit send semantics ([`SendMode::Rendezvous`]
/// checks that a protocol does not rely on eager buffering).
pub fn run_local_with_mode<T, F>(size: usize, timeout: Duration, mode: SendMode, f: F) -> Vec<T>
where
    T: Send,
    F: Fn(Communicator) -> T + Sync,
{
    let comms = Communicator::local_world_with_mode(size, timeout, mode);
    // the caller's debugger, copied into every rank's own scope
    let debugger = akantu_core::debug::current();
    std::thread::scope(|s| {
        let handles: Vec<_> = comms
            .into_iter()
            .map(|c| {
                let f = &f;
                let debugger = &debugger;
                let body = move || {
                    let _rank_debugger = akantu_core::debug::rank_scope(debugger, c.rank(), size);
                    f(c)
                };
                // A rank of a real MPI job runs on its process's main thread
                // (8 MiB stack by default on Linux, `ulimit -s`); the rank
                // threads get the same so the local world runs what the
                // real-MPI binaries run (the default 2 MiB of spawned threads
                // overflowed in debug builds of the parallel cohesive model).
                // `RUST_MIN_STACK` larger than that wins.
                let min = std::env::var("RUST_MIN_STACK")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                std::thread::Builder::new()
                    .stack_size(RANK_THREAD_STACK.max(min))
                    .spawn_scoped(s, body)
                    .unwrap_or_else(|e| panic!("local world: cannot spawn a rank thread: {e}"))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| match h.join() {
                Ok(v) => v,
                Err(p) => std::panic::resume_unwind(p),
            })
            .collect()
    })
}

/// In-flight part of a [`SendRequest`].
#[derive(Debug)]
enum PendingSend {
    /// Synchronous local-world send not yet consumed.
    Local {
        comm: Communicator,
        id: u64,
        dest: Rank,
    },
    /// MPI send (owns its packed bytes).
    #[cfg(feature = "mpi")]
    Mpi {
        comm: Communicator,
        // Boxed: keeps the request enums small (clippy `large_enum_variant`).
        req: Box<crate::mpi_backend::MpiInFlight>,
    },
    /// Synchronous send to self on an MPI communicator, not yet matched
    /// (self channel).
    #[cfg(feature = "mpi")]
    MpiSelf { comm: Communicator, id: u64 },
}

/// Pending non-blocking send owning its buffer (upstream
/// `CommunicationRequest`). Dropping it before completion blocks until the
/// backend no longer accesses the buffer (bounded by the local-world
/// timeout or the MPI deadline; MPI: `MPI_Wait`, leaked on timeout; a
/// failure breaks the communicator's family; on a broken family the
/// request is leaked without any MPI call).
#[derive(Debug)]
#[must_use = "a send request owns its buffer until completion"]
pub struct SendRequest<T: CommData> {
    buffer: Option<Vec<T>>,
    completed: bool,
    pending: Option<PendingSend>,
}

impl<T: CommData> SendRequest<T> {
    fn complete(&mut self) -> Result<()> {
        match self.pending.take() {
            Some(PendingSend::Local { comm, id, dest }) => {
                if let Backend::Local(l) = comm.backend() {
                    l.wait_consumed(id, dest)?;
                }
            }
            #[cfg(feature = "mpi")]
            Some(PendingSend::Mpi { comm, req }) => {
                let bufs = comm.mpi(req.wait())?;
                if self.buffer.is_none() {
                    // a byte buffer moved to the backend (`isend`)
                    self.buffer = bufs
                        .into_iter()
                        .next()
                        .and_then(|b| from_byte_vec::<T>(b).ok());
                }
            }
            #[cfg(feature = "mpi")]
            Some(PendingSend::MpiSelf { comm, id }) => {
                if let Backend::Mpi(m) = comm.backend() {
                    comm.mpi(m.wait_send_to_self(id))?;
                }
            }
            None => {}
        }
        self.completed = true;
        Ok(())
    }

    /// Wait for completion and get the buffer back.
    pub fn wait(mut self) -> Result<Vec<T>> {
        self.complete()?;
        Ok(self.buffer.take().unwrap_or_default())
    }

    /// `true` once completed (non-blocking test).
    pub fn test(&mut self) -> Result<bool> {
        let done = match &mut self.pending {
            None => true,
            Some(PendingSend::Local { comm, id, .. }) => match comm.backend() {
                Backend::Local(l) => l.take_consumed(*id)?,
                _ => true,
            },
            #[cfg(feature = "mpi")]
            Some(PendingSend::Mpi { comm, req }) => {
                let r = req.test();
                comm.mpi(r)?
            }
            #[cfg(feature = "mpi")]
            Some(PendingSend::MpiSelf { comm, id }) => match comm.backend() {
                Backend::Mpi(m) => comm.mpi(m.test_send_to_self(*id))?,
                _ => true,
            },
        };
        if done {
            #[cfg(feature = "mpi")]
            if let Some(PendingSend::Mpi { comm, req }) = self.pending.take()
                && self.buffer.is_none()
            {
                // completed: give back the byte buffer moved to the backend
                let bufs = comm.mpi(req.wait())?;
                self.buffer = bufs
                    .into_iter()
                    .next()
                    .and_then(|b| from_byte_vec::<T>(b).ok());
            }
            self.pending = None;
            self.completed = true;
        }
        Ok(self.completed)
    }
}

impl<T: CommData> Drop for SendRequest<T> {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        match self.pending.take() {
            #[cfg(feature = "mpi")]
            Some(PendingSend::Mpi { comm, mut req }) => {
                if comm.broken().is_some() {
                    // No MPI work on a broken family: keep the buffer and
                    // the guard (the runtime aborts).
                    req.leak();
                } else {
                    // Errors cannot be returned from Drop: the classifier
                    // breaks the family on failure.
                    let _ = comm.mpi(req.wait());
                }
            }
            #[cfg(feature = "mpi")]
            Some(PendingSend::MpiSelf { comm, id }) => {
                // The buffered copy lives in the self channel: wait for the
                // match like an `MPI_Wait` (bounded by the deadline), except
                // on a broken family, where nothing is waited for.
                if comm.broken().is_none()
                    && let Backend::Mpi(m) = comm.backend()
                {
                    let _ = comm.mpi(m.wait_send_to_self(id));
                }
            }
            pending => {
                // The buffer must outlive the backend's access: wait
                // (bounded by the local world's timeout).
                self.pending = pending;
                let _ = self.complete();
            }
        }
    }
}

/// In-flight part of a [`RecvRequest`].
#[derive(Debug)]
enum PendingRecv {
    /// Serial posted slot.
    Serial { slot: u64 },
    /// Local-world posted slot.
    Local { slot: u64 },
    /// Posted receive of the MPI backend (a matching-engine slot, filled
    /// by an exact-length matched receive).
    #[cfg(feature = "mpi")]
    Mpi { slot: u64 },
    /// Posted receive from self on an MPI communicator (self channel).
    #[cfg(feature = "mpi")]
    MpiSelf { slot: u64 },
}

/// Posted non-blocking receive owning its buffer (upstream
/// `asyncReceive`). Dropping it before completion **cancels** it: an
/// unmatched receive is withdrawn (the next matching message goes to later
/// receives); a message already matched to it is consumed and discarded,
/// as with `MPI_Cancel` on a matched receive.
#[derive(Debug)]
#[must_use = "a receive request must be waited on to obtain its data"]
pub struct RecvRequest<T: CommData> {
    comm: Communicator,
    source: Rank,
    tag: Tag,
    len: usize,
    /// Posted size in bytes.
    bytes: usize,
    pending: Option<PendingRecv>,
    _item: std::marker::PhantomData<T>,
}

fn truncated(source: Rank, tag: Tag, posted: usize) -> Error {
    comm_err(format!(
        "message from rank {source} with tag {tag:?} is longer than the posted receive of \
         {posted} bytes (truncated; a wire-contract violation, not a transport failure)"
    ))
}

impl<T: CommData> RecvRequest<T> {
    /// Complete the posted receive: the received bytes (at most the posted
    /// size; a longer message is a truncation error).
    fn complete(&mut self) -> Result<Vec<u8>> {
        let Some(pending) = self.pending.take() else {
            return Err(comm_err("receive request already completed"));
        };
        let bytes = match pending {
            PendingRecv::Serial { slot } => match self.comm.backend() {
                Backend::Serial { state } => {
                    let mut st = state.lock().map_err(|_| comm_err("poisoned mailbox"))?;
                    match st.posted.take(slot) {
                        Some(b) => b,
                        None => {
                            st.posted.cancel(self.tag, slot);
                            return Err(comm_err(format!(
                                "serial receive with tag {:?}: no matching send (would dead-lock)",
                                self.tag
                            )));
                        }
                    }
                }
                _ => return Err(comm_err("receive request of another backend")),
            },
            PendingRecv::Local { slot } => match self.comm.backend() {
                Backend::Local(l) => l.wait_slot(self.source, self.tag, slot)?,
                _ => return Err(comm_err("receive request of another backend")),
            },
            // The whole message is delivered: a longer one is reported as
            // truncated below (never written into a fixed buffer).
            #[cfg(feature = "mpi")]
            PendingRecv::Mpi { slot } => match self.comm.backend() {
                Backend::Mpi(m) => self
                    .comm
                    .mpi(m.wait_posted(slot, self.source, self.tag.0))?,
                _ => return Err(comm_err("receive request of another backend")),
            },
            // The whole self-message is delivered: a longer one is reported
            // as truncated below (never written into a fixed buffer).
            #[cfg(feature = "mpi")]
            PendingRecv::MpiSelf { slot } => match self.comm.backend() {
                Backend::Mpi(m) => self.comm.mpi(m.wait_from_self(self.tag.0, slot))?,
                _ => return Err(comm_err("receive request of another backend")),
            },
        };
        if bytes.len() > self.bytes {
            return Err(truncated(self.source, self.tag, self.bytes));
        }
        Ok(bytes)
    }

    /// Wait for the message (exactly the posted length; a shorter or longer
    /// message is an error, the message is consumed).
    pub fn wait(mut self) -> Result<Vec<T>> {
        let bytes = self.complete()?;
        let mut v = vec![T::zero(); self.len];
        from_bytes(&bytes, &mut v)?;
        Ok(v)
    }

    /// Wait for the message and return it with its **actual length** (at
    /// most the posted length): a length different from the expected one is
    /// a wire-contract violation for the caller to report (a post-check
    /// error), not a transport failure. A message longer than the posted
    /// length is truncated and reported as an ordinary (non-transport)
    /// error.
    pub fn wait_received(mut self) -> Result<Vec<T>> {
        let bytes = self.complete()?;
        match from_byte_vec::<T>(bytes) {
            Ok(v) => Ok(v), // bytes: no copy
            Err(bytes) => vec_from_bytes(&bytes),
        }
    }

    /// Posted length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` if the posted length is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<T: CommData> Drop for RecvRequest<T> {
    fn drop(&mut self) {
        match self.pending.take() {
            None => {}
            Some(PendingRecv::Serial { slot }) => {
                if let Backend::Serial { state } = self.comm.backend()
                    && let Ok(mut st) = state.lock()
                {
                    st.posted.cancel(self.tag, slot);
                }
            }
            Some(PendingRecv::Local { slot }) => {
                if let Backend::Local(l) = self.comm.backend() {
                    l.cancel_slot(self.source, self.tag, slot);
                }
            }
            #[cfg(feature = "mpi")]
            Some(PendingRecv::Mpi { slot }) => {
                if let Backend::Mpi(m) = self.comm.backend() {
                    // Withdrawn if unmatched; a matched message is received
                    // (bounded) and discarded, or leaked on a broken family;
                    // a failure breaks the family.
                    let broken = self.comm.broken().is_some();
                    let _ = self
                        .comm
                        .mpi(m.cancel_posted(slot, self.source, self.tag.0, broken));
                }
            }
            #[cfg(feature = "mpi")]
            Some(PendingRecv::MpiSelf { slot }) => {
                if let Backend::Mpi(m) = self.comm.backend() {
                    m.cancel_from_self(self.tag.0, slot);
                }
            }
        }
    }
}

#[derive(Debug)]
enum BarrierInner {
    Done,
    Local {
        comm: Communicator,
        seq: u64,
    },
    #[cfg(feature = "mpi")]
    Mpi {
        comm: Communicator,
        // Boxed: keeps the request enums small (clippy `large_enum_variant`).
        req: Box<crate::mpi_backend::MpiInFlight>,
    },
}

/// Pending non-blocking barrier (upstream `asyncBarrier`). Dropping an
/// incomplete MPI barrier completes it through the owning communicator's
/// fatal classifier (bounded by the deadline): a failure (e.g. a timeout)
/// marks the communicator's whole family broken, so no new MPI work is
/// admitted on it, its duplicates or its splits; on an
/// already broken family it is leaked without any MPI call.
#[derive(Debug)]
#[must_use = "a barrier request must be completed"]
pub struct BarrierRequest {
    inner: BarrierInner,
}

impl BarrierRequest {
    /// `true` once every rank entered the barrier (non-blocking).
    pub fn test(&mut self) -> Result<bool> {
        let done = match &mut self.inner {
            BarrierInner::Done => true,
            BarrierInner::Local { comm, seq } => match comm.backend() {
                Backend::Local(l) => {
                    let mut st = l.lock()?;
                    let size = l.world.size;
                    let complete = st.barriers.get(seq).is_some_and(|e| e.0 == size);
                    if complete {
                        let e = st.barriers.entry(*seq).or_insert((size, 0));
                        e.1 += 1;
                        if e.1 == size {
                            st.barriers.remove(seq);
                        }
                    }
                    complete
                }
                _ => true,
            },
            #[cfg(feature = "mpi")]
            BarrierInner::Mpi { comm, req } => {
                let r = req.test();
                comm.mpi(r)?
            }
        };
        if done {
            self.inner = BarrierInner::Done;
        }
        Ok(done)
    }

    /// Wait for completion (bounded by the backend's deadline).
    pub fn wait(mut self) -> Result<()> {
        match std::mem::replace(&mut self.inner, BarrierInner::Done) {
            BarrierInner::Done => Ok(()),
            BarrierInner::Local { comm, seq } => {
                let mut req = BarrierRequest {
                    inner: BarrierInner::Local {
                        comm: comm.clone(),
                        seq,
                    },
                };
                let deadline = comm.progress_timeout().map(|t| Instant::now() + t);
                while !req.test()? {
                    if deadline.is_some_and(|d| Instant::now() >= d) {
                        if let Backend::Local(l) = comm.backend() {
                            l.world.timed_out();
                        }
                        return Err(comm_err("non-blocking barrier timed out"));
                    }
                    std::thread::yield_now();
                }
                Ok(())
            }
            #[cfg(feature = "mpi")]
            BarrierInner::Mpi { comm, req } => comm.mpi(req.wait()).map(|_| ()),
        }
    }
}

impl Drop for BarrierRequest {
    fn drop(&mut self) {
        match std::mem::replace(&mut self.inner, BarrierInner::Done) {
            #[cfg(feature = "mpi")]
            BarrierInner::Mpi { comm, mut req } => {
                if comm.broken().is_some() {
                    req.leak();
                } else {
                    let _ = comm.mpi(req.wait());
                }
            }
            // The local world keeps no resource for an abandoned barrier
            // (its arrival stays counted; later barriers use new
            // sequence numbers).
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_secs(10);

    /// The decimal code of the allocation-free notes.
    #[test]
    fn decimal_codes() {
        let mut b = [0u8; 11];
        assert_eq!(decimal(71, &mut b), b"71");
        assert_eq!(decimal(0, &mut b), b"0");
        assert_eq!(decimal(-5, &mut b), b"-5");
        assert_eq!(decimal(i32::MIN, &mut b), b"-2147483648");
        assert_eq!(decimal(i32::MAX, &mut b), b"2147483647");
    }

    /// The note of the abort entry point (static text
    /// and integers, the wrapped error's code) fits its stack buffer for
    /// every rank, and an overlong note is cut, not a panic.
    #[test]
    fn stack_note_of_the_transport_abort() {
        let error = Error::fatal(FatalKind::Transport, Error::Communication("x".into()));
        let note = transport_abort_note(i32::MIN, &error);
        assert_eq!(
            note.as_str(),
            "akantu: rank -2147483648: transport failure (akantu error code 11), \
             aborting the job with exit code 71\n"
        );
        assert!(note.len < STACK_NOTE_CAPACITY, "{} bytes", note.len);
        let mut long = StackNote::new();
        for _ in 0..40 {
            long.text("0123456789");
        }
        assert_eq!(long.as_str().len(), STACK_NOTE_CAPACITY);
    }

    #[test]
    fn serial_collectives_are_identity() {
        let c = Communicator::serial();
        assert_eq!((c.rank(), c.size()), (0, 1));
        let mut v = [1.0f64, 2.0];
        c.all_reduce(&mut v, ReduceOp::Sum).unwrap();
        assert_eq!(v, [1.0, 2.0]);
        assert_eq!(c.all_gather(&[3u32]).unwrap(), vec![3]);
        assert_eq!(c.all_gather_v(&[3u32, 4]).unwrap(), vec![vec![3, 4]]);
        assert_eq!(c.exclusive_scan_sum_u64(5).unwrap(), 0);
        c.barrier().unwrap();
        assert!(c.broadcast(&mut v, 1).is_err());
        c.reduce(&mut v, ReduceOp::Max, 0).unwrap();
        assert_eq!(v, [1.0, 2.0]);
        assert_eq!(c.gather(&[7u8], 0).unwrap(), Some(vec![7]));
        assert_eq!(c.scatter(Some(&[1i32, 2]), 0).unwrap(), vec![1, 2]);
        assert!(c.scatter_v::<i32>(None, 0).is_err());
        assert!(c.ibarrier().unwrap().test().unwrap());
        assert!(c.split(None, 0).unwrap().is_none());
        assert_eq!(c.duplicate().unwrap().size(), 1);
    }

    #[test]
    fn serial_self_messages() {
        let c = Communicator::serial();
        c.send(&[1i64, -2], 0, Tag(7)).unwrap();
        let mut req = c.isend(vec![3i64, 4], 0, Tag(7)).unwrap();
        assert!(req.test().unwrap());
        assert_eq!(
            req.wait().unwrap(),
            vec![3, 4],
            "buffer returned on completion"
        );
        let st = c.probe::<i64>(Some(0), Tag(7)).unwrap();
        assert_eq!((st.source, st.count), (0, 2));
        let mut r = [0i64; 2];
        c.receive(&mut r, 0, Tag(7)).unwrap();
        assert_eq!(r, [1, -2]);
        assert_eq!(
            c.irecv::<i64>(2, 0, Tag(7)).unwrap().wait().unwrap(),
            vec![3, 4]
        );
        assert!(c.receive(&mut r, 0, Tag(7)).is_err());
        assert!(c.iprobe::<i64>(None, Tag(7)).unwrap().is_none());
        let mut wrong = [0i64; 3];
        c.send(&[1i64], 0, Tag(1)).unwrap();
        assert!(c.receive(&mut wrong, 0, Tag(1)).is_err());
        assert!(c.send(&[1u8], 2, Tag(0)).is_err());
        // An operator undefined for the type fails on every backend (the
        // same validation as the local world and MPI).
        assert!(matches!(
            c.all_reduce(&mut [1.0f64], ReduceOp::BitOr),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn local_world_collectives_are_rank_ordered() {
        let out = run_local(3, T, |c| {
            let r = c.rank();
            let mut s = [f64::from(r) + 0.1];
            c.all_reduce(&mut s, ReduceOp::Sum).unwrap();
            let mut m = [r];
            c.all_reduce(&mut m, ReduceOp::Max).unwrap();
            let ex = c
                .exclusive_scan_sum_u64(u64::try_from(r).unwrap() + 1)
                .unwrap();
            let mut inc = [r + 1];
            c.scan(&mut inc, ReduceOp::Sum).unwrap();
            let mut b = [if r == 2 { 42u32 } else { 0 }];
            c.broadcast(&mut b, 2).unwrap();
            let g = c
                .all_gather_v(&vec![r; usize::try_from(r).unwrap()])
                .unwrap();
            c.barrier().unwrap();
            if r == 0 {
                c.send(&[7i32], 1, Tag(3)).unwrap();
            }
            let p2p = if r == 1 {
                let mut x = [0i32];
                c.receive(&mut x, 0, Tag(3)).unwrap();
                x[0]
            } else {
                -1
            };
            (s[0], m[0], ex, inc[0], b[0], g, p2p)
        });
        for (r, o) in out.iter().enumerate() {
            assert_eq!(o.0, ((0.1 + 1.1) + 2.1), "rank-ordered sum");
            assert_eq!(o.1, 2);
            assert_eq!(o.2, [0, 1, 3][r]);
            assert_eq!(o.3, [1, 3, 6][r]);
            assert_eq!(o.4, 42);
            assert_eq!(o.5, vec![vec![], vec![1], vec![2, 2]]);
        }
        assert_eq!(out[1].6, 7);
    }

    #[test]
    fn local_world_rooted_collectives_and_loc_reductions() {
        let out = run_local(3, T, |c| {
            let r = c.rank();
            let mut red = [f64::from(r) * 0.5 + 0.25];
            c.reduce(&mut red, ReduceOp::Sum, 1).unwrap();
            let g = c
                .gather_v(&vec![r; usize::try_from(r).unwrap() + 1], 2)
                .unwrap();
            let blocks: Vec<Vec<u64>> = (0..3u64)
                .map(|k| vec![k; usize::try_from(k).unwrap()])
                .collect();
            let mine = c
                .scatter_v((r == 0).then_some(blocks.as_slice()), 0)
                .unwrap();
            let mut loc = [ValueLoc {
                value: [3.0, 1.0, 1.0][usize::try_from(r).unwrap()],
                loc: 10 - r,
            }];
            c.all_reduce_loc(&mut loc, ReduceOp::MinLoc).unwrap();
            let mut mx = [ValueLoc {
                value: f64::from(r),
                loc: r,
            }];
            c.all_reduce_loc(&mut mx, ReduceOp::MaxLoc).unwrap();
            let bad = c.scatter_v::<u8>(if r == 0 { Some(&[]) } else { None }, 0);
            (red[0], g, mine, loc[0], mx[0], bad.is_err())
        });
        assert_eq!(
            out[1].0,
            (0.25 + 0.75) + 1.25,
            "root holds the rank-ordered sum"
        );
        assert_eq!(out[0].0, 0.25, "non-root unchanged");
        assert_eq!(out[2].1, Some(vec![vec![0], vec![1, 1], vec![2, 2, 2]]));
        assert_eq!(out[0].1, None);
        for (r, o) in out.iter().enumerate() {
            assert_eq!(o.2, vec![r as u64; r]);
            assert_eq!(
                o.3,
                ValueLoc { value: 1.0, loc: 8 },
                "tie keeps the smallest loc"
            );
            assert_eq!(o.4, ValueLoc { value: 2.0, loc: 2 });
            assert!(o.5, "invalid root input fails on every rank");
        }
    }

    #[test]
    fn local_world_split_duplicate_and_probe() {
        let out = run_local(4, T, |c| {
            let r = c.rank();
            let sub = c
                .split((r != 3).then_some(u32::try_from(r % 2).unwrap()), -r)
                .unwrap();
            let sub_info = sub.as_ref().map(|s| {
                let mut v = [u64::try_from(r).unwrap()];
                s.all_reduce(&mut v, ReduceOp::Sum).unwrap();
                (s.rank(), s.size(), v[0])
            });
            let dup = c.duplicate().unwrap();
            assert_eq!((dup.rank(), dup.size()), (r, 4));
            // Any-source probe on the duplicate: rank 0 receives from all.
            if r != 0 {
                dup.send(&[r; 2], 0, Tag(5)).unwrap();
            }
            let mut got = Vec::new();
            if r == 0 {
                for _ in 1..4 {
                    let st = dup.probe::<i32>(None, Tag(5)).unwrap();
                    assert_eq!(st.count, 2);
                    got.push(dup.receive_vec::<i32>(st.source, Tag(5)).unwrap()[0]);
                }
                got.sort_unstable();
            }
            (sub_info, got)
        });
        // color 0: ranks {0, 2} ordered by key -r → 2 then 0.
        assert_eq!(out[0].0, Some((1, 2, 2)));
        assert_eq!(out[2].0, Some((0, 2, 2)));
        assert_eq!(out[1].0, Some((0, 1, 1)));
        assert_eq!(out[3].0, None);
        assert_eq!(out[0].1, vec![1, 2, 3]);
    }

    /// Rendezvous sends complete only on consumption, so a
    /// bidirectional "send all, then receive all" protocol dead-locks
    /// (bounded here by the timeout), while posting owned non-blocking
    /// sends before receiving completes promptly.
    #[test]
    fn rendezvous_mode_exposes_reliance_on_eager_buffering() {
        let t = Duration::from_millis(300);
        let naive = run_local_with_mode(2, t, SendMode::Rendezvous, |c| {
            let peer = 1 - c.rank();
            c.send(&[1.0f64; 4], peer, Tag(9))?;
            let mut r = [0.0f64; 4];
            c.receive(&mut r, peer, Tag(9))
        });
        assert!(
            naive.iter().all(Result::is_err),
            "blocking send-all dead-locks"
        );
        let start = Instant::now();
        let owned = run_local_with_mode(2, Duration::from_secs(10), SendMode::Rendezvous, |c| {
            let peer = 1 - c.rank();
            let recv = c.irecv::<f64>(4, peer, Tag(9))?;
            let send = c.isend(vec![f64::from(c.rank()); 4], peer, Tag(9))?;
            let incoming = recv.wait()?;
            let back = send.wait()?;
            Ok::<_, Error>((incoming, back.len()))
        });
        assert!(start.elapsed() < Duration::from_secs(2));
        for (r, o) in owned.into_iter().enumerate() {
            let (incoming, len) = o.unwrap();
            assert_eq!(incoming, vec![1.0 - r as f64; 4]);
            assert_eq!(len, 4, "buffer returned after completion");
        }
        // Test-before-consumption reports an incomplete request.
        let pending = run_local_with_mode(2, Duration::from_secs(10), SendMode::Rendezvous, |c| {
            if c.rank() == 0 {
                let mut s = c.isend(vec![5u32], 1, Tag(2)).unwrap();
                let early = s.test().unwrap();
                c.barrier().unwrap();
                c.barrier().unwrap();
                (early, s.test().unwrap())
            } else {
                c.barrier().unwrap();
                let mut r = [0u32];
                c.receive(&mut r, 0, Tag(2)).unwrap();
                c.barrier().unwrap();
                (false, r[0] == 5)
            }
        });
        assert_eq!(pending[0], (false, true));
        assert!(pending[1].1);
    }

    #[test]
    fn wait_received_reports_the_actual_length() {
        let out = run_local(2, T, |c| {
            if c.rank() == 0 {
                c.send(&[1u8, 2, 3], 1, Tag(4)).unwrap();
                c.send(&[1u8, 2, 3], 1, Tag(4)).unwrap();
                c.barrier().unwrap();
                (0, true)
            } else {
                let shorter = c
                    .irecv::<u8>(4, 0, Tag(4))
                    .unwrap()
                    .wait_received()
                    .unwrap()
                    .len();
                // Longer than posted: truncated, an ordinary error (not a
                // transport failure), the communicator stays usable.
                let e = c.irecv::<u8>(2, 0, Tag(4)).unwrap().wait_received();
                let ordinary = e.as_ref().is_err_and(|e| {
                    e.fatal_kind().is_none() && e.to_string().contains("truncated")
                });
                c.barrier().unwrap();
                (shorter, ordinary)
            }
        });
        assert_eq!(
            out[1],
            (3, true),
            "length mismatch is the caller's post-check"
        );
    }

    /// Operation, type and root mismatches and an invalid root on the
    /// local world: every contract
    /// violation fails on **every** rank before data moves, promptly, and
    /// the communicator stays usable.
    #[test]
    fn collective_contract_violations_fail_on_every_rank() {
        let start = Instant::now();
        let out = run_local(4, T, |c| {
            let r = c.rank();
            let mut fails = Vec::new();
            // Operator mismatch (silently wrong before the fix).
            let mut x = [r + 1];
            let op = if r == 0 { ReduceOp::Sum } else { ReduceOp::Max };
            fails.push(c.all_reduce(&mut x, op).is_err());
            // Element type mismatch with equal byte sizes.
            fails.push(if r == 0 {
                c.all_reduce(&mut [1.0f64], ReduceOp::Sum).is_err()
            } else {
                c.all_reduce(&mut [1u64], ReduceOp::Sum).is_err()
            });
            // Invalid root on one rank (a local early return before the fix).
            let mut b = [r];
            let root = if r == 0 { 4 } else { 0 };
            fails.push(c.broadcast(&mut b, root).is_err());
            // Every rank names itself root.
            fails.push(c.broadcast(&mut b, r).is_err());
            // Length mismatch in a reduction / fixed gather / rooted reduce.
            let n = if r == 2 { 3 } else { 2 };
            fails.push(c.all_reduce(&mut vec![1.0f64; n], ReduceOp::Sum).is_err());
            fails.push(c.gather(&vec![r; n], 1).is_err());
            fails.push(c.reduce(&mut vec![1i64; n], ReduceOp::Sum, 3).is_err());
            // Operator undefined for the type, and scans.
            fails.push(c.all_reduce(&mut [1.0f32], ReduceOp::BitXor).is_err());
            fails.push(c.scan(&mut vec![1u32; n], ReduceOp::Sum).is_err());
            // Invalid root input of a scatter, invalid split color.
            fails.push(
                c.scatter_v::<u8>(if r == 3 { Some(&[]) } else { None }, 3)
                    .is_err(),
            );
            fails.push(c.split(Some(if r == 1 { u32::MAX } else { 0 }), 0).is_err());
            // Still usable afterwards.
            let mut v = [1u64];
            c.all_reduce(&mut v, ReduceOp::Sum).unwrap();
            (fails, v[0], c.broken().is_none())
        });
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "no rank waited for a timeout"
        );
        for (r, (fails, sum, healthy)) in out.into_iter().enumerate() {
            assert!(fails.iter().all(|&f| f), "rank {r}: {fails:?}");
            assert_eq!(sum, 4);
            assert!(healthy);
        }
    }

    /// Posted receives match in
    /// posting order whatever the completion order.
    #[test]
    fn posted_receives_match_in_posting_order() {
        let out = run_local(4, T, |c| {
            let (r, p) = (c.rank(), 4);
            let (next, prev) = ((r + 1) % p, (r + p - 1) % p);
            let a = c.irecv::<i32>(2, prev, Tag(19)).unwrap();
            let b = c.irecv::<i32>(2, prev, Tag(19)).unwrap();
            let s1 = c.isend(vec![11, 22], next, Tag(19)).unwrap();
            let s2 = c.isend(vec![33, 44], next, Tag(19)).unwrap();
            let second = b.wait_received().unwrap();
            let first = a.wait_received().unwrap();
            s1.wait().unwrap();
            s2.wait().unwrap();
            (first, second)
        });
        for o in out {
            assert_eq!(o, (vec![11, 22], vec![33, 44]));
        }
        // Serial backend: a receive posted before the self-send gets it.
        let c = Communicator::serial();
        let a = c.irecv::<u8>(1, 0, Tag(1)).unwrap();
        let b = c.irecv::<u8>(1, 0, Tag(1)).unwrap();
        c.send(&[1u8], 0, Tag(1)).unwrap();
        c.send(&[2u8], 0, Tag(1)).unwrap();
        assert_eq!(b.wait().unwrap(), vec![2]);
        assert_eq!(a.wait().unwrap(), vec![1]);
        assert!(c.iprobe::<u8>(None, Tag(1)).unwrap().is_none());
    }

    /// A posted receive lets a
    /// blocking synchronous send complete before the receive is waited on.
    #[test]
    fn preposted_receive_completes_a_synchronous_send() {
        let start = Instant::now();
        let out = run_local_with_mode(2, Duration::from_secs(5), SendMode::Rendezvous, |c| {
            let r = c.rank();
            let recv = c.irecv::<i32>(1, 1 - r, Tag(71))?;
            c.send_mode(&[r], 1 - r, Tag(71), SendMode::Rendezvous)?;
            recv.wait()
        });
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(out[0].as_ref().unwrap(), &vec![1]);
        assert_eq!(out[1].as_ref().unwrap(), &vec![0]);
    }

    /// Cancellation semantics of a dropped receive: an unmatched receive is
    /// withdrawn (the message goes to a later receive); a message already
    /// matched to the dropped receive is discarded (as `MPI_Cancel`).
    #[test]
    fn dropped_receive_is_cancelled_or_discards_its_match() {
        let out = run_local(2, T, |c| {
            if c.rank() == 0 {
                c.barrier().unwrap();
                c.send(&[5u8], 1, Tag(3)).unwrap();
                c.barrier().unwrap();
                c.send(&[6u8], 1, Tag(3)).unwrap();
                c.send(&[7u8], 1, Tag(3)).unwrap();
                c.barrier().unwrap();
                (Vec::new(), None)
            } else {
                drop(c.irecv::<u8>(1, 0, Tag(3)).unwrap()); // unmatched
                c.barrier().unwrap();
                let first = c.receive_vec::<u8>(0, Tag(3)).unwrap();
                let matched = c.irecv::<u8>(1, 0, Tag(3)).unwrap();
                c.barrier().unwrap();
                c.barrier().unwrap();
                drop(matched); // already matched with [6]: discarded
                let next = c.receive_vec::<u8>(0, Tag(3)).unwrap();
                (first, Some(next))
            }
        });
        assert_eq!(out[1], (vec![5], Some(vec![7])));
    }

    /// D-039: rank-ordered semantics of exceptional values (±0, NaN, ±inf)
    /// and of MIN/MAX/loc, bit for bit (MPI repeats these under both
    /// policies in `mpi_acceptance`).
    #[test]
    fn rank_ordered_exceptional_values() {
        let out = run_local(3, T, |c| {
            let r = usize::try_from(c.rank()).unwrap();
            let pick = |v: [f64; 3]| v[r];
            let mut v = [
                pick([0.0, -0.0, 0.0]),
                pick([-0.0, 0.0, -0.0]),
                pick([1.0, f64::NAN, 2.0]),
                pick([f64::NAN, 1.0, 2.0]),
                pick([f64::INFINITY, 1.0, f64::NEG_INFINITY]),
            ];
            let mut mn = v;
            let mut mx = v;
            c.all_reduce(&mut v, ReduceOp::Sum).unwrap();
            c.all_reduce(&mut mn, ReduceOp::Min).unwrap();
            c.all_reduce(&mut mx, ReduceOp::Max).unwrap();
            let mut loc = [
                ValueLoc {
                    value: pick([1.0, 0.5, 0.5]),
                    loc: [7, 9, 3][r],
                },
                ValueLoc {
                    value: pick([0.0, -0.0, 0.0]),
                    loc: [2, 1, 0][r],
                },
                ValueLoc {
                    value: pick([f64::NAN, 1.0, 0.0]),
                    loc: [0, 1, 2][r],
                },
            ];
            let mut maxloc = loc;
            c.all_reduce_loc(&mut loc, ReduceOp::MinLoc).unwrap();
            c.all_reduce_loc(&mut maxloc, ReduceOp::MaxLoc).unwrap();
            let bits = |a: &[f64]| a.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            let lbits = |a: &[ValueLoc]| {
                a.iter()
                    .map(|x| (x.value.to_bits(), x.loc))
                    .collect::<Vec<_>>()
            };
            let same = (bits(&v), bits(&mn), bits(&mx), lbits(&loc), lbits(&maxloc));
            (same, loc, maxloc)
        });
        for o in &out {
            assert_eq!(out[0].0, o.0, "every rank gets the same bits");
        }
        let ((sum, mn, mx, _, _), loc, maxloc) = &out[0];
        // Sums: +0 + -0 = +0; -0 + +0 + -0 = +0; NaN propagates; inf - inf = NaN.
        assert_eq!(sum[0], 0.0f64.to_bits());
        assert_eq!(sum[1], 0.0f64.to_bits());
        assert!(f64::from_bits(sum[2]).is_nan() && f64::from_bits(sum[3]).is_nan());
        assert!(f64::from_bits(sum[4]).is_nan());
        // Min/Max keep the accumulated value on ties (+0 stays +0, -0 stays
        // -0) and never replace it by a NaN; a NaN accumulator survives.
        assert_eq!(mn[0], 0.0f64.to_bits());
        assert_eq!(mn[1], (-0.0f64).to_bits());
        assert_eq!(mx[0], 0.0f64.to_bits());
        assert_eq!(mn[2], 1.0f64.to_bits());
        assert_eq!(mx[2], 2.0f64.to_bits());
        assert!(f64::from_bits(mn[3]).is_nan() && f64::from_bits(mx[3]).is_nan());
        assert_eq!(mn[4], f64::NEG_INFINITY.to_bits());
        assert_eq!(mx[4], f64::INFINITY.to_bits());
        // Loc: ties keep the smallest location; a NaN value is never taken.
        assert_eq!(loc[0], ValueLoc { value: 0.5, loc: 3 });
        assert_eq!((loc[1].value.to_bits(), loc[1].loc), (0.0f64.to_bits(), 0));
        assert!(
            loc[2].value.is_nan() && loc[2].loc == 0,
            "a NaN accumulator stays"
        );
        assert_eq!(maxloc[0], ValueLoc { value: 1.0, loc: 7 });
    }

    #[test]
    fn local_world_detects_mismatched_collectives_and_times_out() {
        let out = run_local(2, Duration::from_millis(200), |c| {
            if c.rank() == 0 {
                c.all_reduce(&mut [1u8], ReduceOp::Sum).map(|_| ())
            } else {
                c.barrier()
            }
        });
        assert!(out[0].is_err(), "rank 0 sees a mismatched collective");
        assert!(out[1].is_err(), "rank 1 times out instead of hanging");
    }

    /// MPI-023: finalize handlers run when the last handle is dropped.
    #[test]
    fn finalize_handlers_run_with_the_last_handle() {
        use std::sync::atomic::AtomicUsize;
        let hits = Arc::new(AtomicUsize::new(0));
        let c = Communicator::serial();
        let h = Arc::clone(&hits);
        c.on_finalize(move || {
            h.fetch_add(1, Ordering::SeqCst);
        });
        let c2 = c.clone();
        drop(c);
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        drop(c2);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    /// A transport failure on a communicator breaks its derived ones.
    #[test]
    fn broken_state_is_shared_with_derived_communicators() {
        let c = Communicator::serial();
        let d = c.duplicate().unwrap();
        c.mark_broken("test");
        assert_eq!(
            d.barrier().unwrap_err().fatal_kind(),
            Some(FatalKind::Transport)
        );
    }

    /// NBX with per-round tags (UD-076 contract) on the local world.
    #[test]
    fn receive_any_number_local_world() {
        let out = run_local(4, T, |c| {
            let r = c.rank();
            let mut total = 0;
            for round in 0..20i32 {
                let tag = Tag(100 + round);
                let sends: Vec<SendRequest<i32>> = (0..4)
                    .filter(|&p| p != r && (p + round) % 3 == 0)
                    .map(|p| {
                        c.isend_mode(vec![r * 100 + round], p, tag, SendMode::Rendezvous)
                            .unwrap()
                    })
                    .collect();
                let mut got = Vec::new();
                c.receive_any_number(sends, tag, |src, msg| {
                    assert_eq!(msg, vec![src * 100 + round]);
                    got.push(src);
                    Ok(())
                })
                .unwrap();
                let expected = if (r + round) % 3 == 0 { 3 } else { 0 };
                assert_eq!(got.len(), expected, "rank {r} round {round}");
                total += got.len();
            }
            total
        });
        assert!(out.iter().sum::<usize>() > 0);
    }

    /// `local_timeouts` counts every wait that ended at the world
    /// timeout (receive, non-blocking barrier), on any rank and on worlds
    /// split from the world; nothing else counts. Deterministic: every
    /// counted wait is stranded (nothing can arrive), the healthy operations
    /// are on one rank or need no peer.
    #[test]
    fn local_timeouts_count_waits_that_ended_at_the_timeout() {
        assert_eq!(Communicator::serial().local_timeouts(), None);
        let short = Duration::from_millis(50);
        let c = Communicator::local_world(1, short).pop().unwrap();
        c.barrier().unwrap();
        let half = c.split(Some(0), 0).unwrap().unwrap();
        assert_eq!(
            (c.local_timeouts(), half.local_timeouts()),
            (Some(0), Some(0))
        );
        // nobody sends: both receives end at the timeout
        assert!(c.receive_vec::<u8>(0, Tag(7)).is_err());
        assert_eq!(c.local_timeouts(), Some(1));
        assert!(half.receive_vec::<u8>(0, Tag(7)).is_err());
        assert_eq!(
            (c.local_timeouts(), half.local_timeouts()),
            (Some(2), Some(2))
        );
        // rank 1 never enters the barrier: rank 0's wait ends at the timeout
        let pair = Communicator::local_world(2, short);
        assert!(pair[0].ibarrier().unwrap().wait().is_err());
        assert_eq!(
            (pair[0].local_timeouts(), pair[1].local_timeouts()),
            (Some(1), Some(1))
        );
        // rank 1 never enters the exchange: rank 0's dynamic exchange ends
        // at its progress deadline (the world timeout), counted too
        let nbx = Communicator::local_world(2, short);
        assert!(
            nbx[0]
                .receive_any_number::<u8>(Vec::new(), Tag(9), |_, _| Ok(()))
                .is_err()
        );
        assert_eq!(nbx[0].local_timeouts(), Some(1));
        // a fresh world starts at zero
        assert_eq!(run_local(1, short, |c| c.local_timeouts()), vec![Some(0)]);
    }
}
