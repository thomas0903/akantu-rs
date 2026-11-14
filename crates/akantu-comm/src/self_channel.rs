// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): semantics of point-to-point messages a rank
// sends to itself in src/synchronizer/communicator_mpi_inline_impl.hh
// (sendImpl / receiveImpl / asyncSendImpl / asyncReceiveImpl / probeImpl with
// the own rank). The in-process routing itself is an akantu-rs addition.

//! In-process channel for the messages a rank of an MPI communicator sends
//! to **itself**.
//!
//! The pinned OpenMPI 5.0.7 (pml ob1, btl self) writes the **whole** message
//! past a posted receive buffer when a self-message of at least 4 KiB is
//! longer than the receive (`docs/DEPENDENCIES.md`).
//! The MPI backend therefore never hands a self-message to MPI:
//! every send whose destination is the own rank, every receive and probe
//! whose source is the own rank, and the self part of an any-source probe go
//! through this channel, so no MPI receive can ever match a self-message.
//!
//! Semantics (those of MPI for one source and one tag):
//!
//! * **Posting order.** Messages with one tag match posted receives in
//!   posting order; a message no posted receive is waiting for is queued
//!   and matched by the next posted receive or matched (dynamic) receive
//!   in arrival order. A message delivered to a posted receive is invisible
//!   to probes (as in MPI).
//! * **Truncation** is detected by the caller from the delivered length
//!   (the whole message is kept here): an ordinary error, the message is
//!   consumed (`MPI_ERR_TRUNCATE` semantics), never memory unsafety.
//! * **Send completion.** A standard-mode self-send is buffered (complete at
//!   once, permitted for MPI standard mode). A synchronous self-send
//!   (`MPI_Issend` semantics) completes when a receive matched it: at once
//!   if a posted receive was waiting, otherwise when a later posted receive,
//!   matched receive or cancelled-after-match receive takes it.
//! * **Cancellation.** Withdrawing an unmatched posted receive lets the next
//!   message go to later receives; a message already matched to it is
//!   discarded (`MPI_Cancel` semantics).
//! * **Waiting** never holds the process MPI lock while it blocks; waits
//!   are bounded by the communicator's progress deadline (`None`: wait as
//!   long as MPI would — possibly forever when no thread sends).
//! * **Progress while waiting**. A blocking MPI receive
//!   from self would also progress the rest of MPI (other posted receives,
//!   pending sends). A wait given a [`Progress`] hook therefore waits in
//!   slices ([`crate::pacing::Pacer::slice`]) and runs the hook between
//!   slices with the channel unlocked — the MPI backend matches the
//!   process's posted receives and makes one MPI progress call there. A
//!   hook error ends the wait (a posted receive is withdrawn).
//!
//! The channel is generic (no MPI dependency) so that its semantics are unit
//! tested on every platform.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Instant;

use crate::communicator::PostedSlots;
use crate::pacing::Pacer;

/// Work run between the slices of a channel wait;
/// `None`: block on the channel only. `Ok(true)` reports pending work that
/// needs progress (the wait keeps its short slices, [`Pacer::restart`]); an
/// `Err` ends the wait with [`WaitError::Progress`].
pub(crate) type Progress<'a> = Option<&'a mut dyn FnMut() -> Result<bool, String>>;

/// A queued self-message (no posted receive was waiting for it).
#[derive(Debug)]
struct Queued {
    id: u64,
    bytes: Vec<u8>,
    sync: bool,
}

#[derive(Debug, Default)]
struct State {
    /// Unmatched messages per tag, in arrival order.
    queue: HashMap<i32, VecDeque<Queued>>,
    /// Posted receives per tag (posting order) and their contents.
    posted: PostedSlots<i32>,
    /// Synchronous sends matched by a receive and not yet observed by their
    /// request.
    consumed: HashSet<u64>,
    next_id: u64,
}

impl State {
    /// A queued message was taken by a receive: its synchronous send is
    /// complete.
    fn matched(&mut self, q: Queued) -> Vec<u8> {
        if q.sync {
            self.consumed.insert(q.id);
        }
        q.bytes
    }
}

/// Why a channel wait ended without a result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WaitError {
    /// The progress deadline passed (posted receives are withdrawn).
    Timeout,
    /// The channel's lock was poisoned by a panicking thread.
    Poisoned,
    /// The progress hook failed (posted receives are withdrawn).
    Progress(String),
}

/// The self-message channel of one MPI communicator handle (shared by all
/// clones of that rank's [`crate::Communicator`]).
#[derive(Debug, Default)]
pub(crate) struct SelfChannel {
    state: Mutex<State>,
    changed: Condvar,
}

impl SelfChannel {
    fn lock(&self) -> Result<MutexGuard<'_, State>, WaitError> {
        self.state.lock().map_err(|_| WaitError::Poisoned)
    }

    /// Wait for a change, bounded by `deadline`; `Err(Timeout)` once it
    /// passed. With a `progress` hook the wait lasts at most one slice
    /// (paced by `pacer`); when the slice ends without a change the hook
    /// runs with the channel unlocked, and the caller re-checks its
    /// condition either way.
    fn wait<'a>(
        &'a self,
        g: MutexGuard<'a, State>,
        deadline: Option<Instant>,
        pacer: &mut Pacer,
        progress: &mut Progress<'_>,
    ) -> Result<MutexGuard<'a, State>, WaitError> {
        let now = Instant::now();
        if deadline.is_some_and(|d| now >= d) {
            return Err(WaitError::Timeout);
        }
        let Some(hook) = progress.as_deref_mut() else {
            return match deadline {
                None => self.changed.wait(g).map_err(|_| WaitError::Poisoned),
                Some(d) => self
                    .changed
                    .wait_timeout(g, d - now)
                    .map(|(g, _)| g)
                    .map_err(|_| WaitError::Poisoned),
            };
        };
        let (g, res) = self
            .changed
            .wait_timeout(g, pacer.slice(now, deadline))
            .map_err(|_| WaitError::Poisoned)?;
        if !res.timed_out() {
            return Ok(g);
        }
        drop(g);
        if hook().map_err(WaitError::Progress)? {
            pacer.restart();
        }
        self.lock()
    }

    /// Send `bytes` to self with `tag`. Returns the id of a synchronous send
    /// that is not complete yet (no posted receive was waiting), `None` if
    /// the send is complete.
    pub(crate) fn send(
        &self,
        tag: i32,
        bytes: Vec<u8>,
        sync: bool,
    ) -> Result<Option<u64>, WaitError> {
        let mut st = self.lock()?;
        let pending = match st.posted.deliver(tag, bytes) {
            Ok(()) => None,
            Err(bytes) => {
                let id = st.next_id;
                st.next_id += 1;
                st.queue
                    .entry(tag)
                    .or_default()
                    .push_back(Queued { id, bytes, sync });
                sync.then_some(id)
            }
        };
        drop(st);
        self.changed.notify_all();
        Ok(pending)
    }

    /// `true` once the synchronous send `id` was matched by a receive
    /// (reported once).
    pub(crate) fn test_send(&self, id: u64) -> Result<bool, WaitError> {
        Ok(self.lock()?.consumed.remove(&id))
    }

    /// Wait until the synchronous send `id` was matched by a receive.
    pub(crate) fn wait_send(
        &self,
        id: u64,
        deadline: Option<Instant>,
        mut progress: Progress<'_>,
    ) -> Result<(), WaitError> {
        let mut pacer = Pacer::new();
        let mut st = self.lock()?;
        loop {
            if st.consumed.remove(&id) {
                return Ok(());
            }
            st = self.wait(st, deadline, &mut pacer, &mut progress)?;
        }
    }

    /// Post a receive with `tag`: matched at once by the oldest queued
    /// message, otherwise filled by the next send. Returns the slot.
    pub(crate) fn post(&self, tag: i32) -> Result<u64, WaitError> {
        let mut st = self.lock()?;
        let matched = st.queue.get_mut(&tag).and_then(VecDeque::pop_front);
        let matched = matched.map(|q| st.matched(q));
        let slot = st.posted.post(tag, matched);
        drop(st);
        self.changed.notify_all();
        Ok(slot)
    }

    /// Wait for the message of the posted receive `slot` (whole message,
    /// whatever its length). At the deadline the receive is withdrawn.
    pub(crate) fn wait_posted(
        &self,
        tag: i32,
        slot: u64,
        deadline: Option<Instant>,
        mut progress: Progress<'_>,
    ) -> Result<Vec<u8>, WaitError> {
        let mut pacer = Pacer::new();
        let mut st = self.lock()?;
        loop {
            if let Some(bytes) = st.posted.take(slot) {
                return Ok(bytes);
            }
            st = match self.wait(st, deadline, &mut pacer, &mut progress) {
                Ok(g) => g,
                Err(e) => {
                    if let Ok(mut st) = self.lock() {
                        st.posted.cancel(tag, slot);
                    }
                    return Err(e);
                }
            };
        }
    }

    /// Cancel the posted receive `slot`: withdrawn if unmatched, its
    /// message discarded if matched.
    pub(crate) fn cancel(&self, tag: i32, slot: u64) {
        if let Ok(mut st) = self.lock() {
            st.posted.cancel(tag, slot);
        }
    }

    /// Matched (dynamic) receive: the next queued message with `tag`,
    /// waiting for one bounded by `deadline`.
    pub(crate) fn take(
        &self,
        tag: i32,
        deadline: Option<Instant>,
        mut progress: Progress<'_>,
    ) -> Result<Vec<u8>, WaitError> {
        let mut pacer = Pacer::new();
        let mut st = self.lock()?;
        loop {
            if let Some(q) = st.queue.get_mut(&tag).and_then(VecDeque::pop_front) {
                let bytes = st.matched(q);
                drop(st);
                self.changed.notify_all();
                return Ok(bytes);
            }
            st = self.wait(st, deadline, &mut pacer, &mut progress)?;
        }
    }

    /// Length of the next queued message with `tag` (non-blocking probe).
    pub(crate) fn peek(&self, tag: i32) -> Result<Option<usize>, WaitError> {
        Ok(self
            .lock()?
            .queue
            .get(&tag)
            .and_then(VecDeque::front)
            .map(|q| q.bytes.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn soon() -> Option<Instant> {
        Some(Instant::now() + Duration::from_millis(50))
    }

    #[test]
    fn posted_receives_match_in_posting_order() {
        let ch = SelfChannel::default();
        let a = ch.post(7).unwrap();
        let b = ch.post(7).unwrap();
        assert_eq!(ch.send(7, vec![1], false).unwrap(), None);
        assert_eq!(ch.send(7, vec![2], false).unwrap(), None);
        // Waited in reverse order: matching is by posting order.
        assert_eq!(ch.wait_posted(7, b, soon(), None).unwrap(), vec![2]);
        assert_eq!(ch.wait_posted(7, a, soon(), None).unwrap(), vec![1]);
        // Delivered messages are invisible to probes.
        assert_eq!(ch.peek(7).unwrap(), None);
    }

    #[test]
    fn queued_messages_match_later_receives_in_arrival_order() {
        let ch = SelfChannel::default();
        ch.send(3, vec![1; 5000], false).unwrap();
        ch.send(3, vec![2; 7], false).unwrap();
        ch.send(4, vec![9], false).unwrap();
        assert_eq!(ch.peek(3).unwrap(), Some(5000));
        let s = ch.post(3).unwrap();
        assert_eq!(ch.wait_posted(3, s, soon(), None).unwrap(), vec![1; 5000]);
        assert_eq!(ch.take(3, soon(), None).unwrap(), vec![2; 7]);
        assert_eq!(ch.take(4, soon(), None).unwrap(), vec![9]);
        assert_eq!(ch.take(3, soon(), None), Err(WaitError::Timeout));
    }

    #[test]
    fn long_message_is_delivered_whole_for_the_truncation_check() {
        // The caller compares the delivered length with its posted length:
        // the channel never writes into a fixed-size buffer.
        let ch = SelfChannel::default();
        let s = ch.post(1).unwrap();
        ch.send(1, vec![0x55; 1 << 20], false).unwrap();
        assert_eq!(ch.wait_posted(1, s, soon(), None).unwrap().len(), 1 << 20);
    }

    #[test]
    fn synchronous_send_completes_when_matched() {
        let ch = SelfChannel::default();
        // A posted receive was waiting: complete at once.
        let s = ch.post(2).unwrap();
        assert_eq!(ch.send(2, vec![1], true).unwrap(), None);
        assert_eq!(ch.wait_posted(2, s, soon(), None).unwrap(), vec![1]);
        // Queued: pending until a receive takes it.
        let id = ch.send(2, vec![2], true).unwrap().unwrap();
        assert!(!ch.test_send(id).unwrap());
        assert_eq!(ch.wait_send(id, soon(), None), Err(WaitError::Timeout));
        assert_eq!(ch.take(2, soon(), None).unwrap(), vec![2]);
        assert!(ch.test_send(id).unwrap());
        // Matched by a posted receive later.
        let id = ch.send(2, vec![3], true).unwrap().unwrap();
        let s = ch.post(2).unwrap();
        ch.wait_send(id, soon(), None).unwrap();
        assert_eq!(ch.wait_posted(2, s, soon(), None).unwrap(), vec![3]);
    }

    #[test]
    fn synchronous_send_completes_from_another_thread() {
        let ch = SelfChannel::default();
        let id = ch.send(5, vec![4; 64], true).unwrap().unwrap();
        std::thread::scope(|sc| {
            sc.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                assert_eq!(ch.take(5, None, None).unwrap(), vec![4; 64]);
            });
            ch.wait_send(id, Some(Instant::now() + Duration::from_secs(5)), None)
                .unwrap();
        });
    }

    #[test]
    fn cancel_withdraws_or_discards() {
        let ch = SelfChannel::default();
        // Unmatched: withdrawn, the next message goes to the later receive.
        let a = ch.post(6).unwrap();
        let b = ch.post(6).unwrap();
        ch.cancel(6, a);
        ch.send(6, vec![1], false).unwrap();
        assert_eq!(ch.wait_posted(6, b, soon(), None).unwrap(), vec![1]);
        // Matched: discarded, nothing left to probe.
        let c = ch.post(6).unwrap();
        ch.send(6, vec![2], false).unwrap();
        ch.cancel(6, c);
        assert_eq!(ch.peek(6).unwrap(), None);
        assert_eq!(ch.take(6, soon(), None), Err(WaitError::Timeout));
    }

    #[test]
    fn posted_receive_timeout_withdraws_it() {
        let ch = SelfChannel::default();
        let s = ch.post(8).unwrap();
        assert_eq!(ch.wait_posted(8, s, soon(), None), Err(WaitError::Timeout));
        // The withdrawn receive does not swallow the next message.
        ch.send(8, vec![1], false).unwrap();
        assert_eq!(ch.peek(8).unwrap(), Some(1));
    }

    #[test]
    fn waits_wake_on_a_send_from_another_thread() {
        let ch = SelfChannel::default();
        let s = ch.post(9).unwrap();
        std::thread::scope(|sc| {
            sc.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                ch.send(9, vec![7], false).unwrap();
                std::thread::sleep(Duration::from_millis(20));
                ch.send(9, vec![8], false).unwrap();
            });
            let deadline = Some(Instant::now() + Duration::from_secs(5));
            assert_eq!(ch.wait_posted(9, s, deadline, None).unwrap(), vec![7]);
            assert_eq!(ch.take(9, deadline, None).unwrap(), vec![8]);
        });
    }

    /// A wait with a progress hook runs it between
    /// slices while nothing arrives, and still wakes on a send at once.
    #[test]
    fn waits_run_the_progress_hook_between_slices() {
        let ch = SelfChannel::default();
        let s = ch.post(10).unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|sc| {
            sc.spawn(|| {
                std::thread::sleep(Duration::from_millis(30));
                ch.send(10, vec![5], false).unwrap();
            });
            let mut hook = || {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(false)
            };
            let deadline = Some(Instant::now() + Duration::from_secs(5));
            assert_eq!(
                ch.wait_posted(10, s, deadline, Some(&mut hook)).unwrap(),
                vec![5]
            );
        });
        // 30 ms of 50 µs slices (timer slack makes them longer): many calls.
        assert!(calls.load(std::sync::atomic::Ordering::SeqCst) >= 10);
        // take and wait_send run it too, and keep their deadline.
        let mut n = 0;
        let mut hook = || {
            n += 1;
            Ok(true)
        };
        assert_eq!(
            ch.take(10, soon(), Some(&mut hook)),
            Err(WaitError::Timeout)
        );
        assert!(n > 0);
        let id = ch.send(11, vec![1], true).unwrap().unwrap();
        let mut m = 0;
        let mut hook = || {
            m += 1;
            Ok(false)
        };
        assert_eq!(
            ch.wait_send(id, soon(), Some(&mut hook)),
            Err(WaitError::Timeout)
        );
        assert!(m > 0);
    }

    /// A failing hook ends the wait and withdraws the posted receive.
    #[test]
    fn a_failing_progress_hook_ends_the_wait() {
        let ch = SelfChannel::default();
        let s = ch.post(12).unwrap();
        let mut hook = || Err("MPI progress failed".to_owned());
        assert_eq!(
            ch.wait_posted(12, s, None, Some(&mut hook)),
            Err(WaitError::Progress("MPI progress failed".into()))
        );
        ch.send(12, vec![3], false).unwrap();
        assert_eq!(ch.peek(12).unwrap(), Some(1));
    }
}
