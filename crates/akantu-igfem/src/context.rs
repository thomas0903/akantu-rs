// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the state that upstream's IGFEM materials
// reach through `SolidMechanicsModelIGFEM` (`getFEEngine("IGFEMFEEngine")`,
// `isInside`), extra_packages/igfem/src/solid_mechanics_model_igfem.hh,
// material_igfem/material_igfem.cc.

//! State shared by [`crate::model::SolidMechanicsModelIgfem`] and its IGFEM
//! materials: the IGFEM FE engine (upstream `"IGFEMFEEngine"`, refreshed by
//! the model on every mesh event) and the interface geometry used by the
//! materials' inside tests (upstream `SolidMechanicsModelIGFEM::isInside`).
//! Upstream materials hold a pointer to the model; in akantu-rs the laws are
//! lent mutably by the model during each phase, so the two pieces of model
//! state they read are shared behind locks instead. The locks are never
//! held across a mesh mutation's dispatch by the model.

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use akantu_core::{Error, Real, Result};

use crate::engine::IgfemFeEngine;
use crate::enrichment::InterfaceGeometry;

/// Shared IGFEM model state.
#[derive(Debug)]
pub struct IgfemContext {
    engine: RwLock<IgfemFeEngine>,
    geometry: RwLock<Option<Arc<dyn InterfaceGeometry>>>,
}

fn poisoned(what: &str) -> Error {
    Error::invalid(format!(
        "IGFEM context: the {what} lock is poisoned (a thread panicked)"
    ))
}

impl IgfemContext {
    /// New context around an engine, without geometry.
    #[must_use]
    pub fn new(engine: IgfemFeEngine) -> Self {
        Self {
            engine: RwLock::new(engine),
            geometry: RwLock::new(None),
        }
    }

    /// Read access to the IGFEM FE engine.
    ///
    /// # Errors
    /// Poisoned lock.
    pub fn engine(&self) -> Result<RwLockReadGuard<'_, IgfemFeEngine>> {
        self.engine.read().map_err(|_| poisoned("engine"))
    }

    /// Write access to the IGFEM FE engine.
    ///
    /// # Errors
    /// Poisoned lock.
    pub fn engine_mut(&self) -> Result<RwLockWriteGuard<'_, IgfemFeEngine>> {
        self.engine.write().map_err(|_| poisoned("engine"))
    }

    /// Replace the interface geometry seen by the materials.
    ///
    /// # Errors
    /// Poisoned lock.
    pub fn set_geometry(&self, geometry: Option<Arc<dyn InterfaceGeometry>>) -> Result<()> {
        *self.geometry.write().map_err(|_| poisoned("geometry"))? = geometry;
        Ok(())
    }

    /// The interface geometry, if one is registered.
    ///
    /// # Errors
    /// Poisoned lock.
    pub fn geometry(&self) -> Result<Option<Arc<dyn InterfaceGeometry>>> {
        Ok(self
            .geometry
            .read()
            .map_err(|_| poisoned("geometry"))?
            .clone())
    }

    /// Upstream `SolidMechanicsModelIGFEM::isInside(point, domain)`.
    ///
    /// # Errors
    /// No geometry registered, unknown domain.
    pub fn is_inside(&self, point: &[Real], domain: &str) -> Result<bool> {
        let g = self.geometry()?.ok_or_else(|| {
            Error::invalid(
                "IGFEM: no interface geometry is registered (registerGeometryObject before \
                 creating or updating IGFEM elements)",
            )
        })?;
        g.is_inside(point, domain)
    }
}
