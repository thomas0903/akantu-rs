// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/synchronizer_registry.hh,
// synchronizer_registry.cc.

//! `SynchronizerRegistry` (MPI-015).
//!
//! Upstream keeps a `multimap<SynchronizationTag, Synchronizer*>` plus one
//! registered data accessor and runs every synchronizer registered for a
//! tag, in registration order. Ownership redesign: synchronizers are shared
//! (`Arc`), and the accessor is passed to each call (no stored raw
//! pointer), as everywhere in this crate. Upstream mixes element and node
//! synchronizers in one registry through `dynamic_cast`; here a registry is
//! typed by its entity, so a model keeps one per entity type.

use std::sync::Arc;

use akantu_core::Result;
use akantu_core::enums::SynchronizationTag;

use crate::synchronizer::{DataAccessor, Synchronizer};

/// Synchronizers registered per tag (upstream `SynchronizerRegistry`).
#[derive(Debug)]
pub struct SynchronizerRegistry<E> {
    entries: Vec<(SynchronizationTag, Arc<Synchronizer<E>>)>,
}

impl<E> Default for SynchronizerRegistry<E> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<E: Clone> SynchronizerRegistry<E> {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `synchronizer` for `tag` (upstream
    /// `registerSynchronizer`); a synchronizer may be registered for
    /// several tags, and a tag may have several synchronizers (run in
    /// registration order).
    pub fn register_synchronizer(
        &mut self,
        synchronizer: Arc<Synchronizer<E>>,
        tag: SynchronizationTag,
    ) {
        self.entries.push((tag, synchronizer));
    }

    /// The synchronizers registered for `tag`, in registration order.
    pub fn synchronizers(
        &self,
        tag: SynchronizationTag,
    ) -> impl Iterator<Item = &Arc<Synchronizer<E>>> {
        self.entries
            .iter()
            .filter(move |(t, _)| *t == tag)
            .map(|(_, s)| s)
    }

    /// `true` if nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Run `synchronize(accessor, tag)` on every synchronizer registered
    /// for `tag` (collective; stops at the first — agreed — failure).
    pub fn synchronize(
        &self,
        tag: SynchronizationTag,
        accessor: &mut dyn DataAccessor<E>,
    ) -> Result<()> {
        for s in self.synchronizers(tag) {
            s.synchronize(accessor, tag)?;
        }
        Ok(())
    }

    /// Start `asynchronous_synchronize` on every synchronizer registered for
    /// `tag`. Each pending exchange reserves its communicator until its
    /// `wait_end_synchronize` (one asynchronous exchange per communicator at
    /// a time): synchronizers registered for one tag must use distinct
    /// communicators to be started together.
    pub fn asynchronous_synchronize(
        &self,
        tag: SynchronizationTag,
        accessor: &dyn DataAccessor<E>,
    ) -> Result<()> {
        for s in self.synchronizers(tag) {
            s.asynchronous_synchronize(accessor, tag)?;
        }
        Ok(())
    }

    /// Complete the asynchronous exchanges of every synchronizer registered
    /// for `tag`.
    pub fn wait_end_synchronize(
        &self,
        tag: SynchronizationTag,
        accessor: &mut dyn DataAccessor<E>,
    ) -> Result<()> {
        for s in self.synchronizers(tag) {
            s.wait_end_synchronize(accessor, tag)?;
        }
        Ok(())
    }
}
