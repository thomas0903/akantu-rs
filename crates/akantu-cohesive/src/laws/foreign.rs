// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_material.cc (PyMaterialCohesive
// trampoline: PYBIND11_OVERRIDE of checkInsertion, computeTraction,
// computeTangentTraction; computeStress / computeTangentModuli final no-ops),
// src/model/solid_mechanics/solid_mechanics_model_cohesive/materials/
// material_cohesive.{hh,cc} (the base the subclass derives from: sigma_c,
// delta_c, the cohesive internals, initMaterial).

//! Runtime-defined cohesive laws (D-078; CMAT-001 runtime
//! variant): a [`MaterialCohesive`] whose constitutive behaviour is provided
//! at run time by an object implementing [`ForeignCohesiveMethods`] — the
//! Python trampoline of an `aka.MaterialCohesive` subclass.
//!
//! The law owns everything a built-in cohesive law owns (the common
//! cohesive state [`crate::material::CohesiveCore`]: normals, openings,
//! tractions, `δ_max`, damage, the facet filter, `σ_c` on the facets) plus
//! **runtime** parameters (registered by the subclass, e.g. `G_c`, `beta`)
//! and runtime internals (on the cohesive elements of the law). Its
//! parameters are upstream `MaterialCohesive`'s (`sigma_c`, `delta_c`, and
//! the `Material` ones) followed by the runtime ones; the input file sets
//! both.
//!
//! The call points are the built-in laws': `initMaterial` (default: the
//! base initialisation), `computeTraction(type, ghost)` after the normals
//! and openings of the type are computed, `computeTangentTraction` in the
//! stiffness assembly, and `checkInsertion(check_only)` in the model's
//! insertion check. The behaviour object is taken out of the law for the
//! duration of each call and receives the whole law, so a callback can
//! read and write the law's state (through the binding's lending layer).
//! A law whose behaviour object was taken (a clone, or a re-entrant call)
//! refuses every call instead of computing anything.

use akantu_core::{ElementType, Error, GhostType, Real, Result};
use akantu_model::constitutive_law::LawContext;

use super::InsertionInputs;
use crate::material::MaterialCohesive;

/// Behaviour of a runtime-defined cohesive law (upstream
/// `PyMaterialCohesive`).
pub trait ForeignCohesiveMethods: std::fmt::Debug + Send + Sync {
    /// `initMaterial()`; the default is `super().initMaterial()`
    /// ([`MaterialCohesive::base_init`]).
    ///
    /// # Errors
    /// Those of the behaviour.
    fn init_material(&mut self, law: &mut MaterialCohesive, ctx: &LawContext<'_>) -> Result<()> {
        law.base_init(ctx)
    }

    /// `computeTraction(el_type, ghost_type)`: the tractions of the law's
    /// elements of `(ty, ghost)` from its normals and openings (computed
    /// before the call).
    ///
    /// # Errors
    /// Those of the behaviour.
    fn compute_traction(
        &mut self,
        law: &mut MaterialCohesive,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()>;

    /// `computeTangentTraction(el_type, tangent_matrix, ghost_type)`:
    /// `tangent` holds one `dim × dim` block per quadrature point
    /// (column-major).
    ///
    /// # Errors
    /// Those of the behaviour; upstream `MaterialCohesive`'s default is
    /// `AKANTU_TO_IMPLEMENT`.
    fn compute_tangent_traction(
        &mut self,
        law: &mut MaterialCohesive,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
    ) -> Result<()> {
        let _ = (law, ctx, ty, ghost, tangent);
        Err(Error::NotImplemented(
            "computeTangentTraction of a runtime cohesive law (not defined by it)",
        ))
    }

    /// Whether the law defines its own `checkInsertion`; upstream
    /// `MaterialCohesive::checkInsertion` is `AKANTU_TO_IMPLEMENT`.
    fn has_insertion_check(&self) -> bool;

    /// `checkInsertion(check_only)` inside the model's insertion check,
    /// with the model's insertion inputs. The Python trampoline runs its
    /// check with the model released instead
    /// ([`crate::SolidMechanicsModelCohesive::check_cohesive_stress_prepare`]):
    /// its implementation refuses.
    ///
    /// # Errors
    /// Those of the behaviour.
    fn check_insertion(
        &mut self,
        law: &mut MaterialCohesive,
        inputs: &mut InsertionInputs<'_>,
        check_only: bool,
    ) -> Result<()> {
        let _ = (law, inputs, check_only);
        Err(Error::NotImplemented(
            "checkInsertion of a runtime cohesive law inside the model",
        ))
    }
}

/// The runtime variant of [`super::CohesiveLaw`].
#[derive(Debug)]
pub struct ForeignCohesive {
    /// The behaviour (`None` while it is called, or in a clone).
    pub(crate) methods: Option<Box<dyn ForeignCohesiveMethods>>,
}

impl Clone for ForeignCohesive {
    /// A clone has no behaviour: every call of its law refuses (the
    /// behaviour object of a runtime law is not clonable).
    fn clone(&self) -> Self {
        Self { methods: None }
    }
}

impl ForeignCohesive {
    /// A runtime law with behaviour `methods`.
    #[must_use]
    pub fn new(methods: Box<dyn ForeignCohesiveMethods>) -> Self {
        Self {
            methods: Some(methods),
        }
    }

    /// Whether the behaviour defines its own insertion check (`false` while
    /// the behaviour is taken).
    #[must_use]
    pub fn has_insertion_check(&self) -> bool {
        self.methods
            .as_ref()
            .is_some_and(|m| m.has_insertion_check())
    }
}

fn no_behaviour() -> Error {
    Error::invalid(
        "runtime cohesive law without its behaviour (a re-entrant call or a clone of the law)",
    )
}

impl MaterialCohesive {
    /// Run `f` on the behaviour of a runtime law with the whole law, the
    /// behaviour taken out for the duration of the call.
    pub(crate) fn with_foreign<R>(
        &mut self,
        f: impl FnOnce(&mut dyn ForeignCohesiveMethods, &mut Self) -> Result<R>,
    ) -> Result<R> {
        let mut m = match &mut self.law {
            super::CohesiveLaw::Foreign(fc) => fc.methods.take().ok_or_else(no_behaviour)?,
            _ => {
                return Err(Error::invalid("internal error: not a runtime cohesive law"));
            }
        };
        let r = f(m.as_mut(), self);
        if let super::CohesiveLaw::Foreign(fc) = &mut self.law {
            fc.methods = Some(m);
        }
        r
    }

    /// `true` for a runtime law (a Python `MaterialCohesive` subclass).
    #[must_use]
    pub fn is_foreign(&self) -> bool {
        matches!(self.law, super::CohesiveLaw::Foreign(_))
    }
}
