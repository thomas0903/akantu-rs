// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/{synchronizer.hh,
// synchronizer.cc, synchronizer_tmpl.hh, synchronizer_impl.hh,
// synchronizer_impl_tmpl.hh, communications.hh, communications_tmpl.hh,
// communication_descriptor.hh, communication_descriptor_tmpl.hh,
// data_accessor.hh}.

//! Generic synchronizers and data accessors (MPI-007…014).
//!
//! A [`Synchronizer<E>`] knows, per neighbour rank, which entities `E`
//! (nodes `Idx`, elements [`akantu_core::Element`], DOFs) it **sends** (it
//! owns them, they are ghosts there) and **receives** (ghosts here). Data is
//! moved by a [`DataAccessor<E>`], implemented by whoever owns the data
//! (model, material, FE engine, cohesive inserter…), keyed by a
//! [`SynchronizationTag`].
//!
//! Ownership redesign vs upstream: upstream synchronizers keep raw pointers to
//! registered accessors (`SynchronizerRegistry`). Here accessors are passed
//! explicitly to each call (`synchronize(&mut accessor, tag)`), so borrows are
//! checked at compile time and no registry of pointers is needed; models keep
//! a list of `(tag, which data)` and call the synchronizer themselves
//! ([`crate::registry::SynchronizerRegistry`] runs the synchronizers of a
//! tag). Entity-specific synchronizers that need a distributed mesh
//! (element/node/DOF/facet/periodic/grid, MPI-016…021) build their schemes
//! on top of this type; the generic machinery here is tested with
//! hand-built schemes.
//!
//! # Exchange protocol
//!
//! Every exchange is one [`Phases::run`] phase:
//!
//! 1. **prepack** (local): `nb_data` per neighbour, pack every send buffer,
//!    check the packed size against `nb_data`, compute every receive size —
//!    an accessor error or a size violation is a local preflight failure;
//! 2. **agreement**: all ranks (zero-work ranks included) learn whether
//!    every rank prepacked successfully; on failure nothing is sent;
//! 3. **communication**: post one owned non-blocking receive per incoming
//!    neighbour, then one owned non-blocking send per outgoing neighbour,
//!    complete all receives, then all sends — no reliance on eager
//!    buffering (upstream posts asynchronous communications too); a
//!    transport failure is fatal;
//! 4. **unpack** (local, rank order) with the typed wire contract (received
//!    length == announced `nb_data`, every byte consumed);
//! 5. **post agreement** on the unpack outcome.
//!
//! The asynchronous form ([`Synchronizer::asynchronous_synchronize`] +
//! [`Synchronizer::wait_end_synchronize`], upstream
//! `asynchronousSynchronize`/`waitEndSynchronize`) is the same protocol split
//! in two: steps 1–3 (posting only) in the first call, after which the scope
//! is **parked** in the synchronizer (it stays registered, so the
//! communicator is reserved for the pending exchange: other scopes and raw
//! operations on it are refused until the second call), then completion,
//! unpack and post agreement in the second call. The agreement sequence is
//! identical to the one-shot form.
//!
//! Message tags are namespaced by synchronization tag **and** synchronizer
//! id ([`message_tag`]), so independent synchronizers do not consume each
//! other's messages; within one id and tag, exchanges are ordered by the
//! phase sequence (a round's post agreement completes every receive of the
//! round before any rank posts the next one), which replaces upstream's
//! per-tag message counters (MPI-008).
//!
//! # Debug sanity checks (MPI-014)
//!
//! With [`Synchronizer::set_sanity_checks`], every message starts with a
//! header `(tag, nb_data, destination, source)` verified by the receiver,
//! followed by optional entity-specific check data ([`SanityCheck`],
//! provided by the mesh-aware synchronizers: barycenters, global ids, node
//! flags). Upstream enables this in debug builds; here it is a run-time
//! switch that must be identical on every rank (a mismatch is a wire-contract
//! failure, agreed on every rank, never a hang).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use akantu_core::enums::SynchronizationTag;
use akantu_core::{Error, Idx, Rank, Result};

use crate::accessor::{ReduceDataAccessor, SimpleIdxDataAccessor};
use crate::buffer::CommunicationBuffer;
use crate::communicator::{
    CommData, Communicator, MAX_MESSAGE_BYTES, RecvRequest, ReduceOp, SendRequest, Tag,
    message_bytes,
};
use crate::phase::{ParkedScope, Phases, Severity, digest_bytes, phase_id};

/// Upstream `DataAccessor<E>`: packs/unpacks data attached to entities.
///
/// Contract: for a given `(entities, tag)`, `pack_data` must write exactly
/// `nb_data` bytes and `unpack_data` must read exactly the bytes written by
/// the matching `pack_data` on the remote rank, in the same entity order.
/// `nb_data` must not overflow: compute it with checked arithmetic and
/// return `usize::MAX` on overflow (e.g. [`crate::accessor::record_bytes`]);
/// sizes above [`MAX_MESSAGE_BYTES`] are rejected before anything is
/// allocated, as a preflight failure agreed on every rank.
pub trait DataAccessor<E> {
    /// Bytes needed to communicate `entities` for `tag`.
    fn nb_data(&self, entities: &[E], tag: SynchronizationTag) -> usize;
    /// Pack data of `entities` (sender side).
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[E],
        tag: SynchronizationTag,
    ) -> Result<()>;
    /// Unpack data of `entities` (receiver side).
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        entities: &[E],
        tag: SynchronizationTag,
    ) -> Result<()>;
}

/// Direction of a scheme (upstream `CommunicationSendRecv`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SendRecv {
    /// Entities sent (owned here, ghosts there).
    Send,
    /// Entities received (ghosts here, owned there).
    Recv,
}

/// Send/receive schemes per neighbour rank (upstream `Communications<E>`).
/// `BTreeMap` keeps rank order deterministic.
#[derive(Clone, Debug)]
pub struct Communications<E> {
    /// Entities sent to each rank (owned here, ghost there).
    pub send: BTreeMap<Rank, Vec<E>>,
    /// Entities received from each rank (ghost here, owned there).
    pub recv: BTreeMap<Rank, Vec<E>>,
}

impl<E> Default for Communications<E> {
    fn default() -> Self {
        Self {
            send: BTreeMap::new(),
            recv: BTreeMap::new(),
        }
    }
}

impl<E> Communications<E> {
    /// `true` if there is nothing to exchange.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.send.values().all(Vec::is_empty) && self.recv.values().all(Vec::is_empty)
    }

    /// Schemes of one direction.
    #[must_use]
    pub fn schemes(&self, sr: SendRecv) -> &BTreeMap<Rank, Vec<E>> {
        match sr {
            SendRecv::Send => &self.send,
            SendRecv::Recv => &self.recv,
        }
    }

    /// Mutable schemes of one direction.
    pub fn schemes_mut(&mut self, sr: SendRecv) -> &mut BTreeMap<Rank, Vec<E>> {
        match sr {
            SendRecv::Send => &mut self.send,
            SendRecv::Recv => &mut self.recv,
        }
    }

    /// The scheme for `rank`, created empty if absent (upstream
    /// `createScheme`).
    pub fn scheme_mut(&mut self, rank: Rank, sr: SendRecv) -> &mut Vec<E> {
        self.schemes_mut(sr).entry(rank).or_default()
    }

    /// Empty every scheme (upstream `resetSchemes`).
    pub fn reset(&mut self) {
        self.send.clear();
        self.recv.clear();
    }

    /// Exchange the send and receive schemes (upstream `swapSendRecv`).
    pub fn swap_send_recv(&mut self) {
        std::mem::swap(&mut self.send, &mut self.recv);
    }
}

/// Entity-specific sanity-check data appended after the generic header
/// (MPI-014; upstream `packSanityCheckData`/`unpackSanityCheckData`
/// overrides of the element and node synchronizers).
///
/// `RefUnwindSafe` keeps synchronizers (and the meshes embedding them)
/// usable across `catch_unwind` boundaries.
pub trait SanityCheck<E>: Send + Sync + std::panic::RefUnwindSafe {
    /// Bytes of check data for `entities` and `tag`.
    fn data_size(&self, entities: &[E], tag: SynchronizationTag) -> usize;
    /// Pack the check data (sender side).
    fn pack(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[E],
        tag: SynchronizationTag,
    ) -> Result<()>;
    /// Verify the check data (receiver side; `proc` sent it to `rank`).
    fn unpack(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[E],
        tag: SynchronizationTag,
        proc: Rank,
        rank: Rank,
    ) -> Result<()>;
}

/// Size of the generic sanity header: tag (u32), nb_data (u64),
/// destination (i32), source (i32).
pub const SANITY_HEADER_BYTES: usize = 20;

/// Sanity-check configuration of a synchronizer.
struct Sanity<E> {
    enabled: bool,
    hook: Option<Arc<dyn SanityCheck<E>>>,
}

impl<E> Sanity<E> {
    /// Sanity-check bytes per message (saturating: an overflow is rejected
    /// by [`prepack`]).
    fn extra(&self, entities: &[E], tag: SynchronizationTag) -> usize {
        if !self.enabled {
            return 0;
        }
        SANITY_HEADER_BYTES
            .saturating_add(self.hook.as_ref().map_or(0, |h| h.data_size(entities, tag)))
    }
}

impl<E> Clone for Sanity<E> {
    fn clone(&self) -> Self {
        Self {
            enabled: self.enabled,
            hook: self.hook.clone(),
        }
    }
}

/// An asynchronous exchange between its two halves.
struct PendingExchange {
    scope: ParkedScope,
    recvs: Vec<(Rank, usize, RecvRequest<u8>)>,
    sends: Vec<SendRequest<u8>>,
}

/// An owner → ghost exchange posted inside a running scope
/// ([`Synchronizer::post_in`]) and not yet completed: hand it to
/// [`Synchronizer::wait_in`] of the same synchronizer and scope. Dropping it
/// instead completes (or cancels) the requests without unpacking, and the
/// scope's next agreement no longer matches the peers': always complete it.
#[must_use = "a posted exchange must be completed with Synchronizer::wait_in"]
pub struct PostedExchange {
    tag: SynchronizationTag,
    recvs: Vec<(Rank, usize, RecvRequest<u8>)>,
    sends: Vec<SendRequest<u8>>,
}

impl std::fmt::Debug for PostedExchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostedExchange")
            .field("tag", &self.tag)
            .field("recvs", &self.recvs.len())
            .field("sends", &self.sends.len())
            .finish()
    }
}

/// Root gather/scatter scheme cache (upstream `entities_from_root`,
/// `master_receive_entities`, `entities_changed`; MPI-013).
#[derive(Default)]
struct RootScheme {
    changed: bool,
    /// Local entities this rank contributes, in local order.
    local: Vec<Idx>,
    /// Root only: global ids of each rank's contribution.
    master_receive: Vec<Vec<Idx>>,
}

/// Mutable synchronizer state not copied by `Clone` (pending exchanges,
/// root scheme cache).
#[derive(Default)]
struct State {
    pending: Mutex<Vec<(SynchronizationTag, PendingExchange)>>,
    root: Mutex<RootScheme>,
}

impl State {
    fn fresh() -> Self {
        Self {
            pending: Mutex::new(Vec::new()),
            root: Mutex::new(RootScheme {
                changed: true,
                ..RootScheme::default()
            }),
        }
    }
}

/// Root of the gather/scatter operations (upstream: rank 0, hard-coded).
pub const ROOT: Rank = 0;

/// Entities a rank contributes to root gathers/scatters (upstream
/// `fillEntityToSend`, `localToGlobalEntity`, `canScatterSize`,
/// `gatheredSize`; implemented by the node and DOF synchronizers).
pub trait ScatterGatherEntities {
    /// Local entities owned here (local or master), in local order, and
    /// their global ids (same length).
    fn owned_entities(&self) -> (Vec<Idx>, Vec<Idx>);
    /// Number of local entities (`canScatterSize`).
    fn local_size(&self) -> usize;
    /// Number of global entities (`gatheredSize`; significant on the root).
    fn global_size(&self) -> usize;
}

/// Generic synchronizer over entities `E` (upstream `SynchronizerImpl<E>`).
pub struct Synchronizer<E> {
    /// Identifier (used to build message tags).
    pub id: String,
    /// Communicator.
    pub communicator: Communicator,
    /// Schemes.
    pub communications: Communications<E>,
    sanity: Sanity<E>,
    state: State,
}

impl<E: Clone> Clone for Synchronizer<E> {
    /// Copies id, communicator, schemes and sanity configuration (upstream
    /// copy constructor); pending exchanges and the root scheme cache are
    /// not copied.
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            communicator: self.communicator.clone(),
            communications: self.communications.clone(),
            sanity: self.sanity.clone(),
            state: State::fresh(),
        }
    }
}

impl<E: std::fmt::Debug> std::fmt::Debug for Synchronizer<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Synchronizer")
            .field("id", &self.id)
            .field("communicator", &self.communicator)
            .field("communications", &self.communications)
            .field("sanity_checks", &self.sanity.enabled)
            .field("pending", &self.lock_pending().len())
            .finish()
    }
}

impl<E> Synchronizer<E> {
    fn lock_pending(&self) -> MutexGuard<'_, Vec<(SynchronizationTag, PendingExchange)>> {
        self.state
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<E: Clone> Synchronizer<E> {
    /// Empty synchronizer.
    #[must_use]
    pub fn new(id: impl Into<String>, communicator: Communicator) -> Self {
        Self {
            id: id.into(),
            communicator,
            communications: Communications::default(),
            sanity: Sanity {
                enabled: false,
                hook: None,
            },
            state: State::fresh(),
        }
    }

    /// Enable/disable the debug sanity checks (MPI-014; identical on every
    /// rank).
    pub fn set_sanity_checks(&mut self, enabled: bool) {
        self.sanity.enabled = enabled;
    }

    /// Entity-specific sanity-check data (used when the checks are on).
    pub fn set_sanity_check_hook(&mut self, hook: Arc<dyn SanityCheck<E>>) {
        self.sanity.hook = Some(hook);
    }

    /// `true` if an asynchronous exchange of `tag` is pending.
    #[must_use]
    pub fn has_pending(&self, tag: SynchronizationTag) -> bool {
        self.lock_pending().iter().any(|(t, _)| *t == tag)
    }

    /// Owner → ghost update (upstream `synchronize`/`synchronizeOnce`):
    /// the data of `send[r]` goes to every neighbour `r`, `recv[r]` is
    /// unpacked in rank order (see the module's exchange protocol).
    /// Collective over the communicator; zero-work ranks participate with
    /// empty schemes.
    ///
    /// Owns its scope and settles it before dropping it: a
    /// transport failure returns `Error::Fatal { kind: Transport }` (the
    /// runtime's abort signal; the communicator is then broken), an agreed
    /// fatal failure (an accessor propagating an `Error::Fatal`)
    /// `Error::Fatal { kind: Agreed }` on every rank; ordinary prepack /
    /// unpack errors and a refusal inside another live scope stay
    /// recoverable.
    pub fn synchronize(
        &self,
        accessor: &mut dyn DataAccessor<E>,
        tag: SynchronizationTag,
    ) -> Result<()> {
        let mut ph = Phases::new(&self.communicator, format!("sync:{}", self.id));
        let r = self.synchronize_in(&mut ph, accessor, tag);
        ph.settle(r)
    }

    /// [`Synchronizer::synchronize`] as one phase of an enclosing
    /// collective operation (a mesh-event consumer, a mutation hook):
    /// failures are recorded in `phases`, whose owner settles them.
    pub fn synchronize_in(
        &self,
        phases: &mut Phases,
        accessor: &mut dyn DataAccessor<E>,
        tag: SynchronizationTag,
    ) -> Result<()> {
        self.exchange(phases, Direction::Synchronize, accessor, tag)
    }

    /// Start a non-blocking owner → ghost update (upstream
    /// `asynchronousSynchronize`): prepack, agreement, then post every
    /// receive and send; the scope is parked in the synchronizer (the
    /// communicator stays reserved for this exchange — other scopes and raw
    /// operations on it are refused) until
    /// [`Synchronizer::wait_end_synchronize`] completes it with an accessor
    /// for the same data. Starting a second exchange of the same tag before
    /// completing the first is an error (upstream: "pending receive
    /// communications"). A failure is settled like
    /// [`Synchronizer::synchronize`] and leaves nothing pending.
    pub fn asynchronous_synchronize(
        &self,
        accessor: &dyn DataAccessor<E>,
        tag: SynchronizationTag,
    ) -> Result<()> {
        let mut pending = self.lock_pending();
        if pending.iter().any(|(t, _)| *t == tag) {
            return Err(Error::Communication(format!(
                "synchronizer '{}': an asynchronous exchange of tag {tag} is still pending; \
                 complete it with wait_end_synchronize before starting a new one",
                self.id
            )));
        }
        let mut ph = Phases::new(&self.communicator, format!("sync:{}", self.id));
        match self.post(&mut ph, Direction::Synchronize, accessor, tag) {
            Ok((recvs, sends)) => {
                pending.push((
                    tag,
                    PendingExchange {
                        scope: ph.park(),
                        recvs,
                        sends,
                    },
                ));
                Ok(())
            }
            Err(e) => ph.settle(Err(e)),
        }
    }

    /// Complete an asynchronous update (upstream `waitEndSynchronize`):
    /// resume the parked scope, complete every receive and send, unpack in
    /// rank order with the wire contract, post agreement, settle.
    pub fn wait_end_synchronize(
        &self,
        accessor: &mut dyn DataAccessor<E>,
        tag: SynchronizationTag,
    ) -> Result<()> {
        let pending = {
            let mut p = self.lock_pending();
            match p.iter().position(|(t, _)| *t == tag) {
                Some(i) => p.remove(i).1,
                None => {
                    return Err(Error::Communication(format!(
                        "synchronizer '{}': no pending asynchronous exchange of tag {tag}",
                        self.id
                    )));
                }
            }
        };
        let PendingExchange {
            scope,
            recvs,
            sends,
        } = pending;
        let mut ph = scope.resume();
        let name = exchange_name(&self.id, tag);
        let received = ph.collective(&format!("{name}:complete"), |_comm| complete(recvs, sends));
        let r = match received {
            Ok(received) => {
                let unpacked = unpack_all(
                    &self.communications.recv,
                    received,
                    accessor,
                    tag,
                    &self.sanity,
                    self.communicator.rank(),
                );
                ph.agreed(&format!("{name}:post"), unpacked, Severity::Recoverable)
            }
            Err(e) => Err(e),
        };
        ph.settle(r)
    }

    /// First half of an owner → ghost update inside a running scope
    /// (upstream `asynchronousSynchronize` called from a model step): the
    /// prepack is agreed on every rank (as by [`Synchronizer::synchronize_in`]),
    /// then every receive and send is posted and the exchange is returned
    /// for [`Synchronizer::wait_in`]. Between the two halves the caller does
    /// local work only (no agreement or communication on `phases`) and must
    /// call `wait_in` whatever that work's outcome; it reports a local
    /// failure through `wait_in`'s agreement. On error nothing is posted
    /// (the prepack agreement failed on every rank, or a transport failure).
    pub fn post_in(
        &self,
        phases: &mut Phases,
        accessor: &dyn DataAccessor<E>,
        tag: SynchronizationTag,
    ) -> Result<PostedExchange> {
        let (recvs, sends) = self.post(phases, Direction::Synchronize, accessor, tag)?;
        Ok(PostedExchange { tag, recvs, sends })
    }

    /// Second half of [`Synchronizer::post_in`] (upstream
    /// `waitEndSynchronize`): complete every receive and send, unpack in
    /// rank order with the wire contract (only if `local`, the outcome of
    /// the caller's work since `post_in`, is `Ok`), then one agreement on
    /// the unpacking and `local` together: every rank learns of a failure on
    /// any rank before it uses the received values. The values and their
    /// unpacking order are those of [`Synchronizer::synchronize_in`].
    pub fn wait_in(
        &self,
        phases: &mut Phases,
        posted: PostedExchange,
        accessor: &mut dyn DataAccessor<E>,
        local: Result<()>,
    ) -> Result<()> {
        let PostedExchange { tag, recvs, sends } = posted;
        let name = exchange_name(&self.id, tag);
        let received =
            phases.collective(&format!("{name}:complete"), |_comm| complete(recvs, sends))?;
        let unpacked = local.and_then(|()| {
            unpack_all(
                &self.communications.recv,
                received,
                accessor,
                tag,
                &self.sanity,
                self.communicator.rank(),
            )
        });
        // local (every request completed): under commit points agreed with
        // the scope's next agreement (D-126)
        phases.deferred(&format!("{name}:post"), unpacked, Severity::Recoverable)
    }

    /// Ghost → owner reduction (upstream `slaveReductionOnce`): ghosts pack
    /// (`recv` scheme), owners unpack and reduce (`send` scheme); the
    /// accessor's `unpack_data` implements the reduction for `tag`.
    ///
    /// Owns and settles its scope like [`Synchronizer::synchronize`].
    pub fn slave_reduction(
        &self,
        accessor: &mut dyn DataAccessor<E>,
        tag: SynchronizationTag,
    ) -> Result<()> {
        let mut ph = Phases::new(&self.communicator, format!("sync:{}", self.id));
        let r = self.slave_reduction_in(&mut ph, accessor, tag);
        ph.settle(r)
    }

    /// [`Synchronizer::slave_reduction`] as one phase of an enclosing
    /// collective operation.
    pub fn slave_reduction_in(
        &self,
        phases: &mut Phases,
        accessor: &mut dyn DataAccessor<E>,
        tag: SynchronizationTag,
    ) -> Result<()> {
        self.exchange(phases, Direction::Reduce, accessor, tag)
    }

    // ------------------------------------------------- scheme manipulation

    /// Empty every scheme (upstream `reset`).
    pub fn reset(&mut self) {
        self.communications.reset();
    }

    /// Move the entities of `other`'s schemes satisfying `pred` into this
    /// synchronizer (upstream `split`): for every scheme of `other`, this
    /// synchronizer's scheme for the same rank and direction becomes the
    /// matching entities (in order), `other` keeps the others.
    pub fn split_from(&mut self, other: &mut Synchronizer<E>, mut pred: impl FnMut(&E) -> bool) {
        for sr in [SendRecv::Send, SendRecv::Recv] {
            for (&rank, scheme) in other.communications.schemes_mut(sr).iter_mut() {
                let (matching, rest): (Vec<E>, Vec<E>) =
                    std::mem::take(scheme).into_iter().partition(|e| pred(e));
                *scheme = rest;
                *self.communications.scheme_mut(rank, sr) = matching;
            }
        }
    }

    /// Apply `update(scheme, rank, direction)` to every scheme (upstream
    /// `updateSchemes`).
    pub fn update_schemes(&mut self, mut update: impl FnMut(&mut Vec<E>, Rank, SendRecv)) {
        for sr in [SendRecv::Send, SendRecv::Recv] {
            for (&rank, scheme) in self.communications.schemes_mut(sr).iter_mut() {
                update(scheme, rank, sr);
            }
        }
    }

    /// Exchange send and receive schemes (upstream `swapSendRecv`).
    pub fn swap_send_recv(&mut self) {
        self.communications.swap_send_recv();
    }

    /// Replace the schemes by a copy of `other`'s (upstream `copySchemes` /
    /// `operator=`).
    pub fn copy_schemes(&mut self, other: &Synchronizer<E>) {
        self.communications = other.communications.clone();
    }

    /// Keep the receive entities satisfying `pred` and let their owners
    /// drop the corresponding send entities (upstream `filterScheme`;
    /// collective among neighbours, here one phase of the collective
    /// protocol: every rank sends the kept positions of each non-empty
    /// receive scheme to its owner and filters its non-empty send schemes
    /// with the positions received; positions are validated).
    pub fn filter_scheme(&mut self, pred: impl FnMut(&E) -> bool) -> Result<()> {
        let mut ph = Phases::new(&self.communicator, format!("filter:{}", self.id));
        let r = self.filter_scheme_in(&mut ph, pred);
        ph.settle(r)
    }

    /// [`Synchronizer::filter_scheme`] as one phase of an enclosing
    /// collective operation (the periodic synchronizer update
    /// inside `makePeriodic`); failures are recorded in `phases`, whose
    /// owner settles them. Nothing changes unless the phase succeeded on
    /// every rank.
    pub fn filter_scheme_in(
        &mut self,
        phases: &mut Phases,
        mut pred: impl FnMut(&E) -> bool,
    ) -> Result<()> {
        // Positions travel as `i32`: every message size and position is
        // validated here, in the phase's prepared input, so a rank-local
        // oversize is an agreed recoverable error on every rank, not a
        // transport failure inside the communication step.
        let mut prepared: Result<()> = Ok(());
        let keep: Vec<(Rank, Vec<i32>)> = self
            .communications
            .recv
            .iter()
            .filter(|(_, s)| !s.is_empty())
            .map(|(&r, s)| {
                let positions: Vec<i32> = s
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| pred(e))
                    .map_while(|(i, _)| match i32::try_from(i) {
                        Ok(p) => Some(p),
                        Err(_) => {
                            if prepared.is_ok() {
                                prepared = Err(Error::InvalidArgument(format!(
                                    "filter_scheme: position {i} toward rank {r} exceeds the i32 \
                                     range of the wire format"
                                )));
                            }
                            None
                        }
                    })
                    .collect();
                if prepared.is_ok()
                    && let Err(e) =
                        message_bytes(positions.len(), std::mem::size_of::<i32>(), "filter_scheme")
                {
                    prepared = Err(e);
                }
                (r, positions)
            })
            .collect();
        let incoming: Vec<Rank> = self
            .communications
            .send
            .iter()
            .filter(|(_, s)| !s.is_empty())
            .map(|(&r, _)| r)
            .collect();
        let tag = internal_tag(&self.id, INTERNAL_MODIFY_SCHEME);
        let send_schemes = &self.communications.send;
        let filtered = phases.run("filter_scheme", prepared, |comm, ()| {
            let mut sends = Vec::with_capacity(keep.len());
            for (rank, positions) in &keep {
                sends.push(comm.isend(positions.clone(), *rank, tag)?);
            }
            let mut received = Vec::with_capacity(incoming.len());
            for &rank in &incoming {
                // Dynamic length: matched receive (probe + MPI_Imrecv).
                received.push((rank, comm.receive_vec::<i32>(rank, tag)?));
            }
            for s in sends {
                s.wait()?;
            }
            // Post-processing: validate the positions (strictly increasing,
            // in range) before anything is modified.
            let mut filtered = Vec::with_capacity(received.len());
            for (rank, positions) in received {
                let scheme = send_schemes.get(&rank).map_or(&[][..], Vec::as_slice);
                let mut out = Vec::with_capacity(positions.len());
                let mut last: Option<usize> = None;
                for p in positions {
                    let i = usize::try_from(p).ok().filter(|&i| i < scheme.len()).ok_or_else(|| {
                        Error::Communication(format!(
                            "filter_scheme: rank {rank} sent position {p} outside its scheme ({} entities)",
                            scheme.len()
                        ))
                    });
                    let i = match i {
                        Ok(i) if last.is_none_or(|l| i > l) => i,
                        Ok(_) => {
                            return Ok(Err(Error::Communication(format!(
                                "filter_scheme: positions from rank {rank} are not increasing"
                            ))));
                        }
                        Err(e) => return Ok(Err(e)),
                    };
                    last = Some(i);
                    out.push(scheme[i].clone());
                }
                filtered.push((rank, out));
            }
            Ok(Ok(filtered))
        })?;
        for (rank, positions) in keep {
            if let Some(s) = self.communications.recv.get_mut(&rank) {
                let kept = positions
                    .iter()
                    .filter_map(|&p| usize::try_from(p).ok().and_then(|i| s.get(i).cloned()))
                    .collect();
                *s = kept;
            }
        }
        for (rank, kept) in filtered {
            self.communications.send.insert(rank, kept);
        }
        Ok(())
    }

    // ------------------------------------------------------------ internal

    fn schemes(&self, dir: Direction) -> (&Schemes<E>, &Schemes<E>) {
        match dir {
            Direction::Synchronize => (&self.communications.send, &self.communications.recv),
            Direction::Reduce => (&self.communications.recv, &self.communications.send),
        }
    }

    /// One owner → ghost style exchange (see the module's protocol) with a
    /// **typed wire contract**: the sender's packed size must equal
    /// `nb_data` (UD-035/UD-060: upstream packs fields that are never
    /// unpacked) and the receiver must receive and consume exactly the
    /// bytes it announced.
    fn exchange(
        &self,
        phases: &mut Phases,
        dir: Direction,
        accessor: &mut dyn DataAccessor<E>,
        tag: SynchronizationTag,
    ) -> Result<()> {
        let (send, recv) = self.schemes(dir);
        let mtag = message_tag(&self.id, tag);
        let me = self.communicator.rank();
        let prepared = prepack(send, recv, &*accessor, tag, &self.sanity, me);
        let name = exchange_name(&self.id, tag);
        let sanity = &self.sanity;
        phases.run(&name, prepared, |comm, (out, incoming)| {
            let (recvs, sends) = post_requests(comm, out, incoming, mtag)?;
            let received = complete(recvs, sends)?;
            // Post-processing (inner result): unpack in rank order.
            Ok(unpack_all(recv, received, accessor, tag, sanity, me))
        })
    }

    /// First half of an asynchronous exchange: prepack, agreement, post.
    fn post(
        &self,
        ph: &mut Phases,
        dir: Direction,
        accessor: &dyn DataAccessor<E>,
        tag: SynchronizationTag,
    ) -> Result<Posted> {
        let (send, recv) = self.schemes(dir);
        let mtag = message_tag(&self.id, tag);
        let prepared = prepack(
            send,
            recv,
            accessor,
            tag,
            &self.sanity,
            self.communicator.rank(),
        );
        let name = exchange_name(&self.id, tag);
        let (out, incoming) =
            ph.agreed(&format!("{name}:prepare"), prepared, Severity::Recoverable)?;
        ph.collective(&format!("{name}:send"), |comm| {
            post_requests(comm, out, incoming, mtag)
        })
    }
}

impl Synchronizer<Idx> {
    /// Mark the scatter/gather scheme stale (upstream `entities_changed =
    /// true`): rebuilt by the next gather/scatter (agreed on every rank).
    pub fn set_entities_changed(&self) {
        self.lock_root().changed = true;
    }

    fn lock_root(&self) -> MutexGuard<'_, RootScheme> {
        self.state
            .root
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Rebuild the root scheme if any rank marked it changed (upstream
    /// `hasChanged` + `initScatterGatherCommunicationScheme`).
    fn ensure_root_scheme(
        &self,
        ph: &mut Phases,
        entities: &dyn ScatterGatherEntities,
    ) -> Result<()> {
        let local_flag = self.lock_root().changed;
        let changed = ph.run_collective("root:changed", Ok(local_flag), |c, f| {
            let mut v = [u8::from(f)];
            c.all_reduce(&mut v, ReduceOp::LogicalOr)?;
            Ok(v[0] != 0)
        })?;
        if !changed {
            return Ok(());
        }
        let (local, global) = entities.owned_entities();
        let n_local = entities.local_size();
        let prepared = if local.len() != global.len() {
            Err(Error::InvalidArgument(format!(
                "scatter/gather scheme: {} local entities but {} global ids",
                local.len(),
                global.len()
            )))
        } else if let Some(&bad) = local
            .iter()
            .find(|&&l| usize::try_from(l).map_or(true, |l| l >= n_local))
        {
            Err(Error::InvalidArgument(format!(
                "scatter/gather scheme: local entity {bad} outside 0..{n_local}"
            )))
        } else {
            Ok(global)
        };
        let n_global = entities.global_size();
        let master = ph.run("root:init", prepared, |c, global| {
            let g = c.gather_v(&global, ROOT)?;
            Ok(match g {
                Some(per_rank) => {
                    match per_rank
                        .iter()
                        .flatten()
                        .find(|&&g| usize::try_from(g).map_or(true, |g| g >= n_global))
                    {
                        Some(bad) => Err(Error::InvalidArgument(format!(
                            "scatter/gather scheme: global id {bad} outside 0..{n_global}"
                        ))),
                        // every global entity has one owner
                        // (duplicated node ownership, UD-231, was
                        // accepted silently)
                        None => match duplicate_owner(&per_rank, n_global) {
                            Some((g, a, b)) => Err(Error::InvalidArgument(format!(
                                "scatter/gather scheme: global id {g} owned by ranks {a} and {b}"
                            ))),
                            None => Ok(per_rank),
                        },
                    }
                }
                None => Ok(Vec::new()),
            })
        })?;
        let mut root = self.lock_root();
        root.local = local;
        root.master_receive = master;
        root.changed = false;
        Ok(())
    }

    /// Gather `to_gather` (`nb_component` values per local entity, local
    /// size `entities.local_size()`) on the root, placed by global id
    /// (upstream `gather`, MPI-013; root = rank 0). Returns the gathered
    /// array (`global_size × nb_component`) on the root, `None` elsewhere.
    /// Collective; settled like [`Synchronizer::synchronize`].
    pub fn gather<T: CommData>(
        &self,
        entities: &dyn ScatterGatherEntities,
        to_gather: &[T],
        nb_component: usize,
    ) -> Result<Option<Vec<T>>> {
        if self.communicator.size() == 1 {
            // Upstream: a plain copy on one rank.
            self.check_raw_serial()?;
            return Ok(Some(to_gather.to_vec()));
        }
        let mut ph = Phases::new(&self.communicator, format!("gather:{}", self.id));
        let r = (|| {
            self.ensure_root_scheme(&mut ph, entities)?;
            let local = self.lock_root().local.clone();
            let n_global = entities.global_size();
            // Every size is checked before anything is allocated;
            // a failure is a preflight error agreed everywhere.
            let prepared = sized(entities.local_size(), nb_component, "gather: local array")
                .and_then(|expected| {
                    sized(n_global, nb_component, "gather: global array")?;
                    if to_gather.len() != expected {
                        return Err(Error::InvalidArgument(format!(
                            "gather: array of {} values, expected {} × {nb_component}",
                            to_gather.len(),
                            entities.local_size()
                        )));
                    }
                    let mut vals =
                        Vec::with_capacity(sized(local.len(), nb_component, "gather: values")?);
                    for &l in &local {
                        let r = component_range(l, nb_component, to_gather.len())?;
                        vals.extend_from_slice(&to_gather[r]);
                    }
                    Ok(vals)
                });
            let master = self.lock_root().master_receive.clone();
            ph.run("gather:data", prepared, |c, vals| {
                let g = c.gather_v(&vals, ROOT)?;
                Ok(match g {
                    None => Ok(None),
                    Some(per_rank) => {
                        place_gathered(&per_rank, &master, n_global, nb_component).map(Some)
                    }
                })
            })
        })();
        ph.settle(r)
    }

    /// Scatter `to_scatter` (root only: `global_size × nb_component`
    /// values) into `scattered` (`local_size × nb_component`) by global id,
    /// then synchronize ghosts and slaves from their masters (upstream
    /// `scatter` + `synchronizeArray`; root = rank 0). Collective.
    pub fn scatter<T: CommData>(
        &self,
        entities: &dyn ScatterGatherEntities,
        scattered: &mut [T],
        to_scatter: Option<&[T]>,
        nb_component: usize,
    ) -> Result<()> {
        if self.communicator.size() == 1 {
            self.check_raw_serial()?;
            let src = to_scatter.ok_or_else(|| {
                Error::InvalidArgument("scatter: the root must provide the array".into())
            })?;
            if src.len() != scattered.len() {
                return Err(Error::InvalidArgument(format!(
                    "scatter: {} values for an array of {}",
                    src.len(),
                    scattered.len()
                )));
            }
            scattered.copy_from_slice(src);
            return Ok(());
        }
        let mut ph = Phases::new(&self.communicator, format!("scatter:{}", self.id));
        let r = (|| {
            self.ensure_root_scheme(&mut ph, entities)?;
            let (local, master) = {
                let root = self.lock_root();
                (root.local.clone(), root.master_receive.clone())
            };
            let is_root = self.communicator.rank() == ROOT;
            let n_global = entities.global_size();
            let prepared: Result<Option<Vec<Vec<T>>>> = (|| {
                let expected = sized(entities.local_size(), nb_component, "scatter: local array")?;
                if scattered.len() != expected {
                    return Err(Error::InvalidArgument(format!(
                        "scatter: array of {} values, expected {} × {nb_component}",
                        scattered.len(),
                        entities.local_size()
                    )));
                }
                sized(local.len(), nb_component, "scatter: values")?;
                if !is_root {
                    return Ok(None);
                }
                let global = sized(n_global, nb_component, "scatter: global array")?;
                match to_scatter {
                    Some(src) if src.len() == global => master
                        .iter()
                        .map(|globals| {
                            let mut block = Vec::with_capacity(sized(
                                globals.len(),
                                nb_component,
                                "scatter: block",
                            )?);
                            for &g in globals {
                                block.extend_from_slice(
                                    &src[component_range(g, nb_component, src.len())?],
                                );
                            }
                            Ok(block)
                        })
                        .collect::<Result<Vec<_>>>()
                        .map(Some),
                    Some(src) => Err(Error::InvalidArgument(format!(
                        "scatter: root array of {} values, expected {n_global} × {nb_component}",
                        src.len()
                    ))),
                    None => Err(Error::InvalidArgument(
                        "scatter: the root must provide the array".into(),
                    )),
                }
            })();
            ph.run("scatter:data", prepared, |c, blocks| {
                let mine: Vec<T> = c.scatter_v(blocks.as_deref(), ROOT)?;
                // `local.len() × nb_component` was checked in preparation.
                if Some(mine.len()) != local.len().checked_mul(nb_component) {
                    return Ok(Err(Error::Communication(format!(
                        "scatter: received {} values for {} entities",
                        mine.len(),
                        local.len()
                    ))));
                }
                let len = scattered.len();
                for (l, chunk) in local.iter().zip(mine.chunks(nb_component.max(1))) {
                    match component_range(*l, nb_component, len) {
                        Ok(r) => scattered[r].copy_from_slice(chunk),
                        Err(e) => return Ok(Err(e)),
                    }
                }
                Ok(Ok(()))
            })?;
            let mut acc =
                SimpleIdxDataAccessor::new(scattered, nb_component, SynchronizationTag::Whatever);
            self.synchronize_in(&mut ph, &mut acc, SynchronizationTag::Whatever)
        })();
        ph.settle(r)
    }

    /// Owner → ghost copy of a flat array (upstream `synchronizeArray`:
    /// `synchronizeOnce` with a `SimpleIdxDataAccessor`, tag `_whatever`).
    pub fn synchronize_array<T: CommData>(
        &self,
        array: &mut [T],
        nb_component: usize,
    ) -> Result<()> {
        let mut acc = SimpleIdxDataAccessor::new(array, nb_component, SynchronizationTag::Whatever);
        self.synchronize(&mut acc, SynchronizationTag::Whatever)
    }

    /// Ghost → owner reduction with `op`, then owner → ghost copy
    /// (upstream `reduceSynchronizeArray<Op>`), in one collective scope.
    pub fn reduce_synchronize_array<T: CommData>(
        &self,
        array: &mut [T],
        nb_component: usize,
        op: ReduceOp,
    ) -> Result<()> {
        let mut ph = Phases::new(&self.communicator, format!("sync:{}", self.id));
        let r = self.reduce_synchronize_array_in(&mut ph, array, nb_component, op);
        ph.settle(r)
    }

    /// [`Synchronizer::synchronize_array`] as one phase of an enclosing
    /// collective operation (e.g. a solver callback's
    /// `DofManager::phases`): failures are recorded in `phases`, whose owner
    /// settles them.
    pub fn synchronize_array_in<T: CommData>(
        &self,
        phases: &mut Phases,
        array: &mut [T],
        nb_component: usize,
    ) -> Result<()> {
        let mut acc = SimpleIdxDataAccessor::new(array, nb_component, SynchronizationTag::Whatever);
        self.synchronize_in(phases, &mut acc, SynchronizationTag::Whatever)
    }

    /// [`Synchronizer::reduce_synchronize_array`] as phases of an enclosing
    /// collective operation.
    pub fn reduce_synchronize_array_in<T: CommData>(
        &self,
        phases: &mut Phases,
        array: &mut [T],
        nb_component: usize,
        op: ReduceOp,
    ) -> Result<()> {
        let mut red =
            ReduceDataAccessor::new(&mut *array, nb_component, SynchronizationTag::Whatever, op);
        self.slave_reduction_in(phases, &mut red, SynchronizationTag::Whatever)?;
        let mut acc =
            SimpleIdxDataAccessor::new(&mut *array, nb_component, SynchronizationTag::Whatever);
        self.synchronize_in(phases, &mut acc, SynchronizationTag::Whatever)
    }

    /// One-rank gather/scatter are local copies; still refused like a raw
    /// operation while a scope is live or after a transport failure.
    fn check_raw_serial(&self) -> Result<()> {
        let mut ph = Phases::new(&self.communicator, format!("serial:{}", self.id));
        let r = ph.check("copy");
        ph.settle(r)
    }
}

/// `n × nb_component` values, checked.
fn sized(n: usize, nb_component: usize, what: &str) -> Result<usize> {
    n.checked_mul(nb_component).ok_or_else(|| {
        Error::InvalidArgument(format!("{what}: {n} × {nb_component} values overflow"))
    })
}

/// Value range of entity `e`'s record in an array of `len` values.
fn component_range(e: Idx, nb_component: usize, len: usize) -> Result<std::ops::Range<usize>> {
    usize::try_from(e)
        .ok()
        .and_then(|i| i.checked_mul(nb_component))
        .and_then(|start| start.checked_add(nb_component).map(|end| start..end))
        .filter(|r| r.end <= len)
        .ok_or_else(|| {
            Error::InvalidArgument(format!(
                "entity {e} × {nb_component} components outside an array of {len} values"
            ))
        })
}

/// Place per-rank gathered values at their global ids.
/// First global id listed twice in the per-rank owned ids (all ids already
/// checked to be in `0..n_global`): `(id, first rank, second rank)`.
fn duplicate_owner(per_rank: &[Vec<Idx>], n_global: usize) -> Option<(Idx, usize, usize)> {
    let mut owner: Vec<Option<usize>> = vec![None; n_global];
    for (rank, ids) in per_rank.iter().enumerate() {
        for &g in ids {
            let slot = owner.get_mut(usize::try_from(g).ok()?)?;
            match *slot {
                Some(first) => return Some((g, first, rank)),
                None => *slot = Some(rank),
            }
        }
    }
    None
}

fn place_gathered<T: CommData>(
    per_rank: &[Vec<T>],
    master: &[Vec<Idx>],
    n_global: usize,
    nb_component: usize,
) -> Result<Vec<T>> {
    // Checked on every rank in the gather's preparation.
    let mut out = vec![T::zero(); sized(n_global, nb_component, "gather: global array")?];
    let len = out.len();
    for (rank, (vals, globals)) in per_rank.iter().zip(master).enumerate() {
        if Some(vals.len()) != globals.len().checked_mul(nb_component) {
            return Err(Error::Communication(format!(
                "gather: rank {rank} sent {} values for {} entities",
                vals.len(),
                globals.len()
            )));
        }
        for (&g, chunk) in globals.iter().zip(vals.chunks(nb_component.max(1))) {
            out[component_range(g, nb_component, len)?].copy_from_slice(chunk);
        }
    }
    Ok(out)
}

/// Exchange direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    /// Owners (send scheme) → ghosts (recv scheme).
    Synchronize,
    /// Ghosts (recv scheme) → owners (send scheme).
    Reduce,
}

/// Message tag of an exchange: synchronization tag × 64 + a 6-bit hash of
/// the synchronizer id, offset by 1024 — always within the tag range MPI
/// guarantees (`MPI_TAG_UB >= 32767`). Independent synchronizers with
/// colliding hashes must not exchange the same tag concurrently.
#[must_use]
pub fn message_tag(synchronizer_id: &str, tag: SynchronizationTag) -> Tag {
    let t = u64::try_from(tag as usize).unwrap_or(0) % 400;
    let h = phase_id(synchronizer_id) % 64;
    Tag(i32::try_from(1024 + t * 64 + h).unwrap_or(1024))
}

/// Internal message kinds (upstream `Tag::CommTags` used by the
/// synchronizers outside data exchanges).
const INTERNAL_MODIFY_SCHEME: u64 = 0;

/// Tag of an internal synchronizer message: above every [`message_tag`]
/// (`≤ 26623`) and within `MPI_TAG_UB >= 32767`.
fn internal_tag(synchronizer_id: &str, kind: u64) -> Tag {
    let h = phase_id(synchronizer_id) % 64;
    Tag(i32::try_from(27_000 + kind * 64 + h).unwrap_or(27_000))
}

fn exchange_name(id: &str, tag: SynchronizationTag) -> String {
    format!("{tag}:{:x}", digest_bytes(0, id.as_bytes()))
}

/// Prepacked send buffers and announced receive sizes of one exchange.
type Prepacked = (Vec<(Rank, Vec<u8>)>, Vec<(Rank, usize)>);

/// Schemes of one direction.
type Schemes<E> = BTreeMap<Rank, Vec<E>>;

/// Posted receives `(rank, announced bytes, request)` and sends of one
/// exchange.
type Posted = (Vec<(Rank, usize, RecvRequest<u8>)>, Vec<SendRequest<u8>>);

#[allow(clippy::cast_possible_truncation)] // tag index < 64
fn sanity_header(
    buffer: &mut CommunicationBuffer,
    tag: SynchronizationTag,
    nb: usize,
    dest: Rank,
    src: Rank,
) {
    buffer.pack(&(tag as usize as u32));
    buffer.pack(&u64::try_from(nb).unwrap_or(u64::MAX));
    buffer.pack(&dest);
    buffer.pack(&src);
}

fn prepack<E>(
    send: &Schemes<E>,
    recv: &Schemes<E>,
    accessor: &dyn DataAccessor<E>,
    tag: SynchronizationTag,
    sanity: &Sanity<E>,
    me: Rank,
) -> Result<Prepacked> {
    // Sizes are validated before any allocation: an
    // overflowing or oversized announcement is a preflight failure.
    let size = |entities: &[E], rank: Rank, dir: &str| -> Result<(usize, usize)> {
        let nb = accessor.nb_data(entities, tag);
        nb.checked_add(sanity.extra(entities, tag))
            .filter(|&n| n <= MAX_MESSAGE_BYTES)
            .map(|n| (nb, n))
            .ok_or_else(|| {
                Error::InvalidArgument(format!(
                    "{tag}: the message {dir} rank {rank} ({nb} bytes announced by nb_data, \
                     plus sanity data) exceeds the message limit of {MAX_MESSAGE_BYTES} bytes"
                ))
            })
    };
    let mut out = Vec::new();
    for (&rank, entities) in send {
        if entities.is_empty() {
            continue;
        }
        let (nb, expected) = size(entities, rank, "to")?;
        let mut buffer = CommunicationBuffer::with_capacity(expected);
        if sanity.enabled {
            sanity_header(&mut buffer, tag, nb, rank, me);
            if let Some(h) = &sanity.hook {
                h.pack(&mut buffer, entities, tag)?;
            }
        }
        accessor.pack_data(&mut buffer, entities, tag)?;
        if buffer.len() != expected {
            return Err(Error::Communication(format!(
                "{tag}: packed {} bytes for rank {rank}, nb_data announced {expected}",
                buffer.len()
            )));
        }
        out.push((rank, buffer.into_bytes()));
    }
    let incoming = recv
        .iter()
        .filter(|(_, e)| !e.is_empty())
        .map(|(&rank, e)| size(e, rank, "from").map(|(_, n)| (rank, n)))
        .collect::<Result<_>>()?;
    Ok((out, incoming))
}

fn post_requests(
    comm: &Communicator,
    out: Vec<(Rank, Vec<u8>)>,
    incoming: Vec<(Rank, usize)>,
    mtag: Tag,
) -> Result<Posted> {
    // Post every receive, then every send (owned buffers). Errors here are
    // transport failures (the caller's outer Err).
    let mut recvs = Vec::with_capacity(incoming.len());
    for (rank, n) in incoming {
        recvs.push((rank, n, comm.irecv::<u8>(n, rank, mtag)?));
    }
    let mut sends = Vec::with_capacity(out.len());
    for (rank, bytes) in out {
        sends.push(comm.isend(bytes, rank, mtag)?);
    }
    Ok((recvs, sends))
}

/// One completed receive: `(rank, announced bytes, received bytes or a
/// wire-contract error)`.
type Received = (Rank, usize, Result<Vec<u8>>);

/// Complete every posted receive, then every send. A transport failure is
/// the outer error; a wire-contract failure of one receive (a truncated,
/// longer-than-announced message) is kept with that receive — the other
/// requests are still completed — and reported by [`unpack_all`] as a
/// post-check error.
fn complete(
    recvs: Vec<(Rank, usize, RecvRequest<u8>)>,
    sends: Vec<SendRequest<u8>>,
) -> Result<Vec<Received>> {
    let mut received = Vec::with_capacity(recvs.len());
    for (rank, n, req) in recvs {
        match req.wait_received() {
            Err(e) if e.fatal_kind().is_some() => return Err(e),
            r => received.push((rank, n, r)),
        }
    }
    for s in sends {
        s.wait()?;
    }
    Ok(received)
}

fn unpack_all<E>(
    recv: &Schemes<E>,
    received: Vec<Received>,
    accessor: &mut dyn DataAccessor<E>,
    tag: SynchronizationTag,
    sanity: &Sanity<E>,
    me: Rank,
) -> Result<()> {
    // Nothing is unpacked if any receive violated the wire contract.
    let received = received
        .into_iter()
        .map(|(rank, n, r)| {
            r.map(|b| (rank, n, b))
                .map_err(|e| Error::Communication(format!("{tag}: receive from rank {rank}: {e}")))
        })
        .collect::<Result<Vec<_>>>()?;
    for (rank, expected, bytes) in received {
        if bytes.len() != expected {
            return Err(Error::Communication(format!(
                "{tag}: received {} bytes from rank {rank}, nb_data announced {expected}",
                bytes.len()
            )));
        }
        let entities = recv
            .get(&rank)
            .ok_or_else(|| Error::Communication(format!("{tag}: unexpected rank {rank}")))?;
        let mut buffer = CommunicationBuffer::from_bytes(bytes);
        if sanity.enabled {
            let t: u32 = buffer.unpack()?;
            let nb: u64 = buffer.unpack()?;
            let dest: Rank = buffer.unpack()?;
            let src: Rank = buffer.unpack()?;
            let own_nb = accessor.nb_data(entities, tag);
            if t as usize != tag as usize
                || usize::try_from(nb).ok() != Some(own_nb)
                || dest != me
                || src != rank
            {
                return Err(Error::Communication(format!(
                    "{tag}: sanity header from rank {rank} mismatch (tag {t}, nb_data {nb}, \
                     to {dest}, from {src}; expected nb_data {own_nb}, to {me}, from {rank})"
                )));
            }
            if let Some(h) = &sanity.hook {
                h.unpack(&mut buffer, entities, tag, rank, me)?;
            }
        }
        accessor.unpack_data(&mut buffer, entities, tag)?;
        if buffer.remaining() != 0 {
            return Err(Error::Communication(format!(
                "{tag}: {} bytes from rank {rank} were not consumed by unpack",
                buffer.remaining()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Noop;
    impl DataAccessor<u32> for Noop {
        fn nb_data(&self, e: &[u32], _: SynchronizationTag) -> usize {
            e.len() * 8
        }
        fn pack_data(
            &self,
            b: &mut CommunicationBuffer,
            e: &[u32],
            _: SynchronizationTag,
        ) -> Result<()> {
            for _ in e {
                b.pack(&0.0f64);
            }
            Ok(())
        }
        fn unpack_data(
            &mut self,
            b: &mut CommunicationBuffer,
            e: &[u32],
            _: SynchronizationTag,
        ) -> Result<()> {
            for _ in e {
                b.unpack::<f64>()?;
            }
            Ok(())
        }
    }

    #[test]
    fn serial_synchronize_without_schemes_is_noop() {
        let s: Synchronizer<u32> = Synchronizer::new("nodes", Communicator::serial());
        assert!(s.communications.is_empty());
        s.synchronize(&mut Noop, SynchronizationTag::SmmUv).unwrap();
        s.slave_reduction(&mut Noop, SynchronizationTag::SmmRes)
            .unwrap();
        let mut b = CommunicationBuffer::new();
        Noop.pack_data(&mut b, &[1, 2], SynchronizationTag::Test)
            .unwrap();
        assert_eq!(b.len(), Noop.nb_data(&[1, 2], SynchronizationTag::Test));
    }

    /// Owner rank 0 sends values of its nodes to ghost copies on rank 1.
    struct Values {
        v: Vec<f64>,
        cheat: bool,
    }
    impl DataAccessor<u32> for Values {
        fn nb_data(&self, e: &[u32], _: SynchronizationTag) -> usize {
            e.len() * 8
        }
        fn pack_data(
            &self,
            b: &mut CommunicationBuffer,
            e: &[u32],
            _: SynchronizationTag,
        ) -> Result<()> {
            for &n in e {
                b.pack(&self.v[n as usize]);
            }
            if self.cheat {
                b.pack(&0u8); // more bytes than announced (UD-035-like)
            }
            Ok(())
        }
        fn unpack_data(
            &mut self,
            b: &mut CommunicationBuffer,
            e: &[u32],
            _: SynchronizationTag,
        ) -> Result<()> {
            for &n in e {
                self.v[n as usize] = b.unpack()?;
            }
            Ok(())
        }
    }

    /// A rank-specific packing error is agreed before anything is
    /// sent (no rank waits for a message), and a bidirectional exchange
    /// completes with rendezvous sends (no reliance on eager buffering).
    #[test]
    fn packing_failure_is_agreed_and_rendezvous_exchange_completes() {
        use std::time::{Duration, Instant};
        struct Failing(bool);
        impl DataAccessor<u32> for Failing {
            fn nb_data(&self, e: &[u32], _: SynchronizationTag) -> usize {
                e.len() * 8
            }
            fn pack_data(
                &self,
                b: &mut CommunicationBuffer,
                e: &[u32],
                _: SynchronizationTag,
            ) -> Result<()> {
                if self.0 {
                    return Err(Error::Numerical("injected packing error".into()));
                }
                for &n in e {
                    b.pack(&f64::from(n));
                }
                Ok(())
            }
            fn unpack_data(
                &mut self,
                b: &mut CommunicationBuffer,
                e: &[u32],
                _: SynchronizationTag,
            ) -> Result<()> {
                for _ in e {
                    b.unpack::<f64>()?;
                }
                Ok(())
            }
        }
        let t0 = Instant::now();
        let out = crate::run_local(2, Duration::from_secs(20), |c| {
            let mut s: Synchronizer<u32> = Synchronizer::new("nodes", c.clone());
            if c.rank() == 0 {
                s.communications.send.insert(1, vec![0]);
            } else {
                s.communications.recv.insert(0, vec![0]);
            }
            let r = s.synchronize(&mut Failing(c.rank() == 0), SynchronizationTag::Test);
            (r.map_err(|e| e.to_string()), Instant::now())
        });
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "completed long before the timeout"
        );
        assert!(
            out[0]
                .0
                .as_ref()
                .unwrap_err()
                .contains("injected packing error")
        );
        assert!(
            out[1]
                .0
                .as_ref()
                .unwrap_err()
                .contains("failed on another rank")
        );
        // Bidirectional exchange of 1 MiB per direction, rendezvous sends.
        let t0 = Instant::now();
        let out = crate::run_local_with_mode(
            2,
            Duration::from_secs(20),
            crate::SendMode::Rendezvous,
            |c| {
                let peer = 1 - c.rank();
                let mut s: Synchronizer<u32> = Synchronizer::new("big", c.clone());
                let ids: Vec<u32> = (0..131_072).collect();
                s.communications.send.insert(peer, ids.clone());
                s.communications.recv.insert(peer, ids);
                let mut acc = Values {
                    v: vec![f64::from(c.rank()); 131_072],
                    cheat: false,
                };
                s.synchronize(&mut acc, SynchronizationTag::SmmUv)
                    .map(|()| acc.v[7])
            },
        );
        assert!(t0.elapsed() < Duration::from_secs(10));
        assert_eq!(
            (
                out[0].as_ref().copied().unwrap(),
                out[1].as_ref().copied().unwrap()
            ),
            (1.0, 0.0)
        );
    }

    /// Prepare agreement with an absent rank: an empty synchronizer on a
    /// two-rank world, rank 1 never enters the exchange, rank 0 calls the
    /// standalone wrapper with a no-op accessor. Rank 0's prepare
    /// agreement times out — a transport failure — and the wrapper now
    /// settles its scope before dropping it: `Error::Fatal { kind:
    /// Transport }` (and the communicator is broken), for both
    /// `synchronize` and `slave_reduction`.
    #[test]
    fn standalone_wrappers_return_the_transport_abort_signal() {
        use akantu_core::FatalKind;
        use std::time::Duration;
        for reduction in [false, true] {
            let out = crate::run_local(2, Duration::from_millis(250), |c| {
                let s: Synchronizer<u32> = Synchronizer::new("empty", c.clone());
                if c.rank() == 1 {
                    return (None, false);
                }
                let r = if reduction {
                    s.slave_reduction(&mut Noop, SynchronizationTag::SmmRes)
                } else {
                    s.synchronize(&mut Noop, SynchronizationTag::SmmUv)
                };
                let e = r.unwrap_err();
                assert!(e.to_string().contains("timed out"), "{e}");
                (e.fatal_kind(), c.broken().is_some())
            });
            assert_eq!(
                out[0],
                (Some(FatalKind::Transport), true),
                "reduction: {reduction}"
            );
        }
    }

    /// Error classification: ordinary prepack errors and a refusal
    /// inside another live scope stay recoverable; an accessor propagating
    /// an `Error::Fatal { kind: Agreed }` on one rank is agreed fatal and
    /// returned as such on every rank.
    #[test]
    fn standalone_wrappers_keep_recoverable_and_agreed_classes() {
        use akantu_core::FatalKind;
        use std::time::Duration;
        struct Packing(Option<Option<FatalKind>>);
        impl DataAccessor<u32> for Packing {
            fn nb_data(&self, e: &[u32], _: SynchronizationTag) -> usize {
                e.len() * 8
            }
            fn pack_data(
                &self,
                b: &mut CommunicationBuffer,
                e: &[u32],
                _: SynchronizationTag,
            ) -> Result<()> {
                if let Some(kind) = self.0 {
                    let err = Error::Numerical("injected packing error".into());
                    return Err(match kind {
                        Some(k) => Error::fatal(k, err),
                        None => err,
                    });
                }
                for _ in e {
                    b.pack(&0.0f64);
                }
                Ok(())
            }
            fn unpack_data(
                &mut self,
                b: &mut CommunicationBuffer,
                e: &[u32],
                _: SynchronizationTag,
            ) -> Result<()> {
                for _ in e {
                    b.unpack::<f64>()?;
                }
                Ok(())
            }
        }
        for kind in [None, Some(FatalKind::Agreed)] {
            let out = crate::run_local(2, Duration::from_secs(5), |c| {
                let mut s: Synchronizer<u32> = Synchronizer::new("nodes", c.clone());
                if c.rank() == 0 {
                    s.communications.send.insert(1, vec![0]);
                } else {
                    s.communications.recv.insert(0, vec![0]);
                }
                let mut acc = Packing((c.rank() == 0).then_some(kind));
                let e = s
                    .synchronize(&mut acc, SynchronizationTag::Test)
                    .unwrap_err();
                (e.fatal_kind(), c.broken().is_none())
            });
            assert_eq!(out, vec![(kind, true), (kind, true)], "{kind:?}");
        }
        // Refused inside a live scope: local, recoverable, nothing sent.
        let out = crate::run_local(2, Duration::from_secs(5), |c| {
            let _owner = Phases::new(&c, "owner");
            let s: Synchronizer<u32> = Synchronizer::new("nodes", c.clone());
            let e = s
                .slave_reduction(&mut Noop, SynchronizationTag::SmmRes)
                .unwrap_err();
            (e.fatal_kind(), e.to_string().contains("already active"))
        });
        assert_eq!(out, vec![(None, true), (None, true)]);
    }

    #[test]
    fn independent_synchronizers_use_distinct_tags() {
        assert_ne!(
            message_tag("nodes", SynchronizationTag::SmmUv),
            message_tag("elements", SynchronizationTag::SmmUv)
        );
        let t = message_tag("x", SynchronizationTag::Whatever);
        assert!(t.0 >= 1024 && t.0 <= 32767);
        let i = internal_tag("x", INTERNAL_MODIFY_SCHEME);
        assert!(i.0 > 1024 + 399 * 64 + 63 && i.0 <= 32767);
    }

    #[test]
    fn two_rank_exchange_and_wire_contract() {
        use std::time::Duration;
        for cheat in [false, true] {
            let out = crate::run_local(2, Duration::from_millis(300), |c| {
                let mut s: Synchronizer<u32> = Synchronizer::new("nodes", c.clone());
                if c.rank() == 0 {
                    s.communications.send.insert(1, vec![2, 0]);
                } else {
                    s.communications.recv.insert(0, vec![0, 1]);
                }
                let mut acc = Values {
                    v: vec![10.0 + f64::from(c.rank()), 20.0, 30.0],
                    cheat,
                };
                let r = s.synchronize(&mut acc, SynchronizationTag::SmmUv);
                (r.is_ok(), acc.v)
            });
            if cheat {
                assert!(!out[0].0, "sender detects the size violation");
                assert!(!out[1].0, "receiver does not hang");
            } else {
                assert!(out[0].0 && out[1].0);
                assert_eq!(out[1].1, vec![30.0, 10.0, 30.0]);
            }
        }
    }

    /// An overflowing or oversized announced size on one
    /// rank (here: rank-specific `nb_component`) is a preflight failure
    /// agreed on every rank, before anything is allocated or posted; the
    /// communicator stays usable. Also run with `--release` (no overflow
    /// checks) by the fast tier's release pass.
    #[test]
    fn rank_specific_size_overflow_is_agreed_without_allocation() {
        use crate::{ReduceOp, SimpleIdxDataAccessor};
        use std::time::{Duration, Instant};
        for (bad_rank, nb) in [(1, usize::MAX / 2), (0, usize::MAX / 16 + 1), (2, 1 << 40)] {
            let t0 = Instant::now();
            let out = crate::run_local(3, Duration::from_secs(5), |c| {
                let r = c.rank();
                let p = 3;
                let mut s: Synchronizer<Idx> = Synchronizer::new("ovf", c.clone());
                s.communications.send.insert((r + 1) % p, vec![0, 1]);
                s.communications.recv.insert((r + p - 1) % p, vec![2]);
                let mut data = vec![f64::from(r); 3];
                let nb_component = if r == bad_rank { nb } else { 1 };
                let mut acc = SimpleIdxDataAccessor::new(
                    &mut data,
                    nb_component,
                    SynchronizationTag::Whatever,
                );
                let e = s.synchronize(&mut acc, SynchronizationTag::Whatever);
                let mut v = [1u8];
                let usable = c.all_reduce(&mut v, ReduceOp::Sum).is_ok() && v[0] == 3;
                (e.err().map(|e| (e.fatal_kind(), e.to_string())), usable)
            });
            assert!(
                t0.elapsed() < Duration::from_secs(3),
                "agreed, not timed out"
            );
            for (r, (e, usable)) in out.into_iter().enumerate() {
                let (kind, msg) = e.unwrap_or_else(|| panic!("rank {r} must fail ({bad_rank})"));
                assert_eq!(kind, None, "recoverable on rank {r}: {msg}");
                assert!(usable, "rank {r}");
            }
        }
    }

    /// A message longer than the announced (posted) size
    /// is truncated — a wire-contract failure reported by the post
    /// agreement on every rank, never a transport failure; the other
    /// requests are still completed.
    #[test]
    fn truncated_receive_is_a_post_check_failure() {
        use std::time::Duration;
        struct Liar;
        impl DataAccessor<u32> for Liar {
            fn nb_data(&self, e: &[u32], _: SynchronizationTag) -> usize {
                e.len() * 8
            }
            fn pack_data(
                &self,
                b: &mut CommunicationBuffer,
                e: &[u32],
                _: SynchronizationTag,
            ) -> Result<()> {
                for &n in e {
                    b.pack(&f64::from(n));
                }
                Ok(())
            }
            fn unpack_data(
                &mut self,
                b: &mut CommunicationBuffer,
                e: &[u32],
                _: SynchronizationTag,
            ) -> Result<()> {
                for _ in e {
                    b.unpack::<f64>()?;
                }
                Ok(())
            }
        }
        let out = crate::run_local(2, Duration::from_secs(5), |c| {
            let mut s: Synchronizer<u32> = Synchronizer::new("liar", c.clone());
            if c.rank() == 0 {
                s.communications.send.insert(1, vec![1, 2, 3]);
            } else {
                // The receiver expects 2 entities (16 bytes): 24 arrive.
                s.communications.recv.insert(0, vec![1, 2]);
            }
            let r = s.synchronize(&mut Liar, SynchronizationTag::SmmUv);
            let again = c.barrier().is_ok();
            (r.err().map(|e| (e.fatal_kind(), e.to_string())), again)
        });
        for (r, (e, again)) in out.into_iter().enumerate() {
            let (kind, msg) = e.unwrap_or_else(|| panic!("rank {r} must fail"));
            assert_eq!(kind, None, "rank {r}: {msg}");
            assert!(again, "rank {r} usable");
        }
    }
}
