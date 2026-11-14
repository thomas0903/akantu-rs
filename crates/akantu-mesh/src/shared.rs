// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_event_handler_manager.hh
// (registration of mesh event handlers with priorities), src/mesh/mesh.cc:
// 105-150 (Mesh::sendEvent<NewNodesEvent>: bbox, flags, groups, then
// handlers), src/model/solid_mechanics/solid_mechanics_model_cohesive/
// solid_mechanics_model_cohesive.cc:49-94 (CohesiveMeshGlobalDataUpdater:
// flags, synchronizers, global ids, elements event before nodes event,
// participation by global counts). Replaces the `Mesh &` references held by
// Model, FEEngine, DOFManager, synchronizers and dumpers.

//! [`SharedMesh`]: the mesh **domain** — the single owner of a mesh, its
//! facet mesh and the registry of every component whose state depends on
//! its topology (decision record D-019).
//!
//! * Models, FE engines, DOF managers, dumpers and the Python wrapper hold a
//!   cheap handle (`SharedMesh = Arc<MeshDomain>`) and a
//!   [`ConsumerToken`] obtained by [`SharedMesh::register_consumer`]. They
//!   read the mesh per operation with [`SharedMesh::read`] and never store
//!   `&Mesh`.
//! * Topology changes happen **only** through [`SharedMesh::mutate`], a
//!   staged transaction (see [`crate::mutation`]) that must be given the
//!   handler of **every** registered consumer; it updates mesh-owned
//!   dependents, agrees collectively at each stage, dispatches the
//!   committed events in global priority order and records one
//!   acknowledgement per consumer and revision.
//! * A failure before commit rolls the mesh back on every rank (no
//!   half-mutated mesh); a failure while consumers update **poisons** the
//!   domain on every rank: further reads, mutations and solves fail.
//! * No lock is ever waited for: re-entrant or concurrent mutation attempts
//!   (a consumer or Python callback mutating during dispatch, another thread
//!   holding a read guard) fail promptly with `Error::Mesh` instead of
//!   dead-locking — a re-entrant attempt is refused **locally, before any
//!   collective**. Poisoned `std` locks are never silently
//!   recovered. The write lock is held from preflight until every consumer
//!   processed the committed events, so reads from other threads fail
//!   promptly during a whole mutation.
//! * Per-mesh state of crates above the mesh that every model on the mesh
//!   must share (one object per domain, e.g. `akantu_io`'s group-dumper
//!   registry) hangs off the
//!   domain as a typed attachment ([`SharedMesh::attachment`]); it lives as
//!   long as the domain and takes no part in mutations.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError, Weak};

use akantu_core::enums::EventPriority;
use akantu_core::{Error, Result};

use crate::mesh::Mesh;

/// Which mesh of the domain a consumer depends on (upstream: handlers
/// registered on the mesh or on `mesh_facets`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MeshPart {
    /// The mesh itself.
    Main,
    /// Its facet mesh.
    Facets,
}

/// Registry id of a consumer (registration order; identical on every rank
/// when components are created in the same order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConsumerId(pub u64);

#[derive(Clone, Debug)]
pub(crate) struct ConsumerInfo {
    pub(crate) name: String,
    pub(crate) priorities: Vec<EventPriority>,
    pub(crate) part: MeshPart,
    pub(crate) acked_revision: u64,
}

/// Lifecycle phase of the domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Idle,
    Mutating,
    Dispatching,
}

#[derive(Debug)]
pub(crate) struct DomainState {
    pub(crate) consumers: BTreeMap<ConsumerId, ConsumerInfo>,
    pub(crate) next_id: u64,
    pub(crate) phase: Phase,
    /// Poisoning record: kind (the runtime's abort signal) and
    /// reason.
    pub(crate) poisoned: Option<(akantu_core::FatalKind, String)>,
    /// Revision of the last committed mutation.
    pub(crate) committed: u64,
}

impl DomainState {
    /// The [`Error::Fatal`] every use of a poisoned domain returns.
    pub(crate) fn poison_error(&self) -> Option<Error> {
        self.poisoned
            .as_ref()
            .map(|(kind, p)| Error::fatal(*kind, Error::Mesh(format!("mesh domain poisoned: {p}"))))
    }
}

/// Owner of a mesh, its facet mesh and its consumer registry.
#[derive(Debug)]
pub struct MeshDomain {
    pub(crate) mesh: RwLock<Mesh>,
    pub(crate) state: Mutex<DomainState>,
    /// The mesh communicator (fixed for the mesh's lifetime), available
    /// without taking the mesh lock so that a busy or poisoned domain can
    /// still join agreements.
    pub(crate) comm: akantu_comm::Communicator,
    /// Typed attachments of other crates, at most one per type
    /// ([`SharedMesh::attachment`]).
    pub(crate) attachments: Mutex<Vec<Arc<dyn Any + Send + Sync>>>,
}

/// Shared handle on a [`MeshDomain`] (cheap to clone).
#[derive(Clone, Debug)]
pub struct SharedMesh(pub(crate) Arc<MeshDomain>);

/// Proof of registration of one consumer; unregisters on drop.
#[derive(Debug)]
pub struct ConsumerToken {
    id: ConsumerId,
    part: MeshPart,
    domain: Weak<MeshDomain>,
}

impl ConsumerToken {
    /// Registry id.
    #[must_use]
    pub fn id(&self) -> ConsumerId {
        self.id
    }

    /// Mesh part the consumer depends on.
    #[must_use]
    pub fn part(&self) -> MeshPart {
        self.part
    }

    /// `true` if the token belongs to `mesh`.
    #[must_use]
    pub fn belongs_to(&self, mesh: &SharedMesh) -> bool {
        std::ptr::eq(self.domain.as_ptr(), Arc::as_ptr(&mesh.0))
    }
}

impl Drop for ConsumerToken {
    fn drop(&mut self) {
        if let Some(d) = self.domain.upgrade()
            && let Ok(mut s) = d.state.lock()
        {
            s.consumers.remove(&self.id);
        }
    }
}

pub(crate) fn lock_err(what: &str) -> Error {
    Error::Mesh(format!(
        "{what}: lock poisoned by a panic; the mesh domain is unusable"
    ))
}

impl SharedMesh {
    /// Take ownership of `mesh` (no consumers yet).
    #[must_use]
    pub fn new(mesh: Mesh) -> Self {
        let comm = mesh.communicator().clone();
        Self(Arc::new(MeshDomain {
            comm,
            mesh: RwLock::new(mesh),
            state: Mutex::new(DomainState {
                consumers: BTreeMap::new(),
                next_id: 0,
                phase: Phase::Idle,
                poisoned: None,
                committed: 0,
            }),
            attachments: Mutex::new(Vec::new()),
        }))
    }

    pub(crate) fn state(&self) -> Result<std::sync::MutexGuard<'_, DomainState>> {
        self.0
            .state
            .lock()
            .map_err(|_| lock_err("mesh domain state"))
    }

    /// Shared read access for the duration of one operation. Fails promptly
    /// if the domain is poisoned or its topology is being modified (never
    /// waits for a writer).
    pub fn read(&self) -> Result<RwLockReadGuard<'_, Mesh>> {
        if let Some(e) = self.state()?.poison_error() {
            return Err(e);
        }
        match self.0.mesh.try_read() {
            Ok(g) => Ok(g),
            Err(TryLockError::WouldBlock) => Err(Error::Mesh(
                "mesh topology is being modified (re-entrant or concurrent access)".into(),
            )),
            Err(TryLockError::Poisoned(_)) => Err(lock_err("mesh")),
        }
    }

    pub(crate) fn try_write(&self) -> Result<RwLockWriteGuard<'_, Mesh>> {
        match self.0.mesh.try_write() {
            Ok(g) => Ok(g),
            Err(TryLockError::WouldBlock) => Err(Error::Mesh(
                "cannot mutate the mesh: it is borrowed (a read guard is alive)".into(),
            )),
            Err(TryLockError::Poisoned(_)) => Err(lock_err("mesh")),
        }
    }

    /// Register a single-level consumer of `part` with upstream event
    /// priority `priority`. Registering during a mutation is an error.
    pub fn register_consumer(
        &self,
        name: impl Into<String>,
        priority: EventPriority,
        part: MeshPart,
    ) -> Result<ConsumerToken> {
        self.register_multilevel_consumer(name, &[priority], part)
    }

    /// Register a consumer taking part at several priority levels (a model:
    /// FE engines 9, DOF manager 20, fields/laws 94, non-local 100).
    pub fn register_multilevel_consumer(
        &self,
        name: impl Into<String>,
        priorities: &[EventPriority],
        part: MeshPart,
    ) -> Result<ConsumerToken> {
        let mut priorities = priorities.to_vec();
        priorities.sort();
        priorities.dedup();
        if priorities.is_empty() {
            return Err(Error::invalid(
                "a mesh consumer needs at least one priority level",
            ));
        }
        let mut s = self.state()?;
        if s.phase != Phase::Idle {
            return Err(Error::Mesh(
                "cannot register a mesh consumer during a mutation".into(),
            ));
        }
        let id = ConsumerId(s.next_id);
        s.next_id += 1;
        let committed = s.committed;
        s.consumers.insert(
            id,
            ConsumerInfo {
                name: name.into(),
                priorities,
                part,
                acked_revision: committed,
            },
        );
        Ok(ConsumerToken {
            id,
            part,
            domain: Arc::downgrade(&self.0),
        })
    }

    /// Revision of the last committed mutation.
    pub fn revision(&self) -> Result<u64> {
        Ok(self.state()?.committed)
    }

    /// Check that `token`'s consumer processed every committed mutation
    /// (called by models at the start of `solve_step`).
    pub fn check_current(&self, token: &ConsumerToken) -> Result<()> {
        let s = self.state()?;
        if let Some(e) = s.poison_error() {
            return Err(e);
        }
        match s.consumers.get(&token.id) {
            Some(c) if c.acked_revision == s.committed => Ok(()),
            Some(c) => Err(Error::Mesh(format!(
                "consumer '{}' is at mesh revision {}, mesh is at {}",
                c.name, c.acked_revision, s.committed
            ))),
            None => Err(Error::Mesh("unregistered mesh consumer".into())),
        }
    }

    /// Poisoning reason, if any.
    pub fn poisoned(&self) -> Result<Option<String>> {
        Ok(self.state()?.poisoned.as_ref().map(|(_, p)| p.clone()))
    }

    /// Class of the poisoning, if any ([`akantu_core::FatalKind::Transport`]
    /// — this rank observed a transport failure, the runtime must abort;
    /// [`akantu_core::FatalKind::Agreed`] — poisoned on every rank).
    pub fn poison_kind(&self) -> Result<Option<akantu_core::FatalKind>> {
        Ok(self.state()?.poisoned.as_ref().map(|(k, _)| *k))
    }

    /// Names of registered consumers (registration order).
    pub fn consumer_names(&self) -> Result<Vec<String>> {
        Ok(self
            .state()?
            .consumers
            .values()
            .map(|c| c.name.clone())
            .collect())
    }

    /// `true` if both handles refer to the same domain.
    #[must_use]
    pub fn ptr_eq(&self, other: &SharedMesh) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// The domain's attachment of type `T`, created by `init` on the first
    /// request: every caller, through any handle on this domain, gets the
    /// same object (one per type and domain), dropped with the domain.
    /// Crates above the mesh use it for per-mesh state that all models on
    /// the mesh must share (e.g. `akantu_io::dumper::group::SharedMeshDumpers`);
    /// a crate should attach a private type so that only it can reach the
    /// object. Attachments are not consumers: mutations ignore them. `init`
    /// runs without any lock held; when two threads race, one result is
    /// kept and the other dropped.
    ///
    /// # Errors
    /// The attachment lock was poisoned by a panic.
    pub fn attachment<T: Any + Send + Sync>(&self, init: impl FnOnce() -> T) -> Result<Arc<T>> {
        let find = |list: &[Arc<dyn Any + Send + Sync>]| {
            list.iter().find_map(|a| Arc::clone(a).downcast::<T>().ok())
        };
        let lock = || {
            self.0
                .attachments
                .lock()
                .map_err(|_| lock_err("mesh domain attachments"))
        };
        if let Some(t) = find(&lock()?) {
            return Ok(t);
        }
        let fresh = Arc::new(init());
        let mut list = lock()?;
        if let Some(t) = find(&list) {
            return Ok(t);
        }
        list.push(Arc::clone(&fresh) as Arc<dyn Any + Send + Sync>);
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachments_are_one_per_type_and_domain() {
        #[derive(Debug)]
        struct A(u32);
        #[derive(Debug)]
        struct B;
        let m = SharedMesh::new(Mesh::new(2, "m").unwrap());
        let a1 = m.attachment(|| A(1)).unwrap();
        let a2 = m.clone().attachment(|| A(2)).unwrap();
        assert!(Arc::ptr_eq(&a1, &a2));
        assert_eq!(a2.0, 1);
        let _b = m.attachment(|| B).unwrap();
        assert!(Arc::ptr_eq(&a1, &m.attachment(|| A(3)).unwrap()));
        // another domain has its own
        let other = SharedMesh::new(Mesh::new(2, "m").unwrap());
        assert_eq!(other.attachment(|| A(4)).unwrap().0, 4);
        // dropped with the domain
        let weak = Arc::downgrade(&a1);
        drop((a1, a2, m));
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn registration_and_reads() {
        let m = SharedMesh::new(Mesh::new(3, "m").unwrap());
        let m2 = m.clone();
        assert!(m.ptr_eq(&m2));
        assert_eq!(m.read().unwrap().spatial_dimension(), 3);
        let t = m
            .register_consumer("fe", EventPriority::FE_ENGINE, MeshPart::Main)
            .unwrap();
        assert!(t.belongs_to(&m2));
        m.check_current(&t).unwrap();
        assert_eq!(m.consumer_names().unwrap(), vec!["fe"]);
        drop(t);
        assert!(m.consumer_names().unwrap().is_empty());
        let _g = m.read().unwrap();
        assert!(
            m.try_write().is_err(),
            "writer does not wait for a live reader"
        );
    }
}
