// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/non_local_manager_igfem.{hh,cc}
// (NonLocalManagerIGFEM).

//! The IGFEM non-local manager (IGFEM-009, upstream `NonLocalManagerIGFEM`).
//!
//! Upstream derives `NonLocalManagerIGFEM` from `NonLocalManager` to make
//! the non-local toolbox see the integration points of **both** element
//! kinds: its constructor and `init` allocate the per-point positions,
//! volumes and jacobians of the `_ek_regular` (model FE engine) and
//! `_ek_igfem` (`"IGFEMFEEngine"`) kinds, `computeAllNonLocalStresses`
//! flattens the local variables of both kinds, and `onElementsAdded` /
//! `onElementsRemoved` resize / compact the IGFEM per-point arrays before
//! the base class handles the event.
//!
//! In akantu-rs the non-local manager takes its per-point layout from
//! **point sources**, one per element kind
//! ([`akantu_model::non_local::NonLocalPointSource`]); [`IgfemPointSource`]
//! is the IGFEM kind (point counts and integration weights of the IGFEM FE
//! engine). The IGFEM model installs it on its solid model
//! ([`akantu_solid::model::SolidMechanicsPhysics::extra_point_sources`]),
//! so that the solid model's manager is upstream's IGFEM manager:
//!
//! * the layout, volumes and jacobians cover both kinds
//!   (`NonLocalManager::initialize_with_sources`); a point of either kind
//!   inserted in a neighbourhood is averaged with the weights of both
//!   kinds' neighbours;
//! * the model services count the IGFEM points, so the local variables of
//!   the IGFEM laws (`grad_u`, …) are flattened and inflated with the IGFEM
//!   point counts (upstream `flattenInternal(…, _ek_igfem)`; without it the
//!   solid model would flatten them with the regular engine's counts and
//!   refuse a model that mixes IGFEM laws and a non-local law averaging an
//!   internal the IGFEM laws also have);
//! * element events of the IGFEM kind invalidate the neighbourhoods (the
//!   element-added / -removed hook: upstream resizes its IGFEM arrays and
//!   leaves the neighbourhoods as they are, UD-562); the IGFEM model
//!   rebuilds them after every interface update
//!   (`crate::model::SolidMechanicsModelIgfem::update`, feature `cgal`).
//!
//! Which points enter a neighbourhood is decided by the laws (upstream
//! `insertQuadsInNeighborhoods` of the non-local laws); upstream has no
//! non-local IGFEM law, so with upstream's laws the neighbourhoods hold
//! regular points only and IGFEM points are averaged only by a law (or a
//! callback) that inserts them — the test of this module does, against a
//! brute-force weighted average. Upstream's `distributeInternals(_ek_regular)`
//! copies the averages back to the regular kind only; the solid model
//! copies them to its non-local laws (all regular), which is the same with
//! upstream's laws (UD-1209). `cleanupExtraGhostElements` belongs to the
//! parallel manager (D-085: parallel IGFEM is an approved exception).

use std::sync::Arc;

use akantu_core::{ElementKind, ElementType, GhostType, Real, Result};
use akantu_mesh::Mesh;
use akantu_model::non_local::NonLocalPointSource;

use crate::context::IgfemContext;

/// The IGFEM-kind point source of the non-local manager: the IGFEM FE
/// engine's point counts and integration weights (upstream
/// `initElementTypeMap(…, fee_igfem, _ek_igfem)`, `setJacobians(fee_igfem,
/// _ek_igfem)`).
#[derive(Debug, Clone)]
pub struct IgfemPointSource {
    /// The shared IGFEM model state (its engine).
    pub ctx: Arc<IgfemContext>,
}

impl NonLocalPointSource for IgfemPointSource {
    fn kind(&self) -> ElementKind {
        ElementKind::Igfem
    }

    fn nb_integration_points(&self, ty: ElementType, _ghost: GhostType) -> Result<usize> {
        self.ctx.engine()?.nb_integration_points(ty)
    }

    fn jacobians(&self, mesh: &Mesh, ty: ElementType, ghost: GhostType) -> Result<Vec<Real>> {
        Ok(self
            .ctx
            .engine()?
            .jacobians(mesh, ty, ghost)?
            .as_slice()
            .to_vec())
    }
}
