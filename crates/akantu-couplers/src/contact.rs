// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/model_couplers/coupler_solid_contact.hh,
// coupler_solid_contact.cc, coupler_solid_contact_tmpl.hh,
// coupler_solid_cohesive_contact.hh, coupler_solid_cohesive_contact.cc,
// cohesive_contact_solvercallback.hh, cohesive_contact_solvercallback.cc.

//! Monolithic solid–contact couplers (CPL-001…004).
//!
//! # Structure (ARCHITECTURE, monolithic style)
//!
//! Upstream's `CouplerSolidContactTemplate<SMM>` is a `Model` that creates
//! **one** DOF manager and builds the solid model and the contact model on
//! it; the coupler is the `SolverCallback` of the shared time-step solvers.
//! In the port the solid model owns the `ModelSolver`; the coupler replaces
//! it at construction by the coupler's own (DOF manager
//! `<coupler id>:dof_manager_default`, solvers of the coupler's
//! `model coupler_solid_contact [ … ]` section — upstream ignores the
//! solid's own section when a DOF manager is passed) and drives it with a
//! combined callback, [`SolidContactCallback`], that calls the parts in
//! upstream order:
//!
//! | callback | upstream (`coupler_solid_contact_tmpl.hh`) |
//! |---|---|
//! | `assemble_residual` | explicit lumped: positions ← solid current positions, `contact.search()`; solid then contact internal forces; `r += f_ext`, `r += f_int`, `r += f_contact` |
//! | `assemble_residual_part` | `external`: `f_ext + f_contact`; `internal`: `f_int` (stored values, not recomputed) |
//! | `assemble_matrix("K")` | `solid.assembleStiffnessMatrix(true)`, then the contact stiffness for static / implicit dynamic (the cohesive couplings: see UD-772 below) |
//! | `assemble_matrix("M")`, lumped `M` | the solid's |
//! | `predictor` | the solid's (the port's corrected default also leaves a contact search owed for static / implicit dynamic: UD-1291 below) |
//! | `corrector` | the solid's, then for static / implicit dynamic positions ← current, `contact.search()` (CPL-002: the Newton loop is also the active-set loop; the friction history: UD-374 below) |
//! | `before/after_solve_step` | solid then contact (UD-374: a converged static / implicit step commits the friction history in between) |
//! | `matrix_type` | `K`, `M` symmetric, others not defined |
//!
//! The solid's own `assembleResidual` is not called by the coupler, so the
//! solid's external work is not accumulated under the coupler (upstream
//! neither).
//!
//! [`CouplerSolidCohesiveContact`] (CPL-003) is the same coupler with a
//! [`SolidMechanicsModelCohesive`] (id `solid_mechanics_model_cohesive`; its
//! internal forces compute the cohesive tractions first) and the contact
//! model on the **facet mesh**. The surface selectors that follow inserted
//! cohesive elements (`CohesiveSurfaceSelector`, `AllSurfaceSelector`) are
//! mesh-event handlers upstream; the port forwards the new cohesive
//! elements to the contact model ([`CohesiveForwarder`]) before every solve
//! step and after every insertion through the coupler (the next search is
//! the first reader, so the order of operations is upstream's). Insertions
//! are forwarded per batch with the connectivity of the forward: if two
//! insertion batches doubled nodes before one forward, the facets of the
//! first batch would be read with the later connectivity, where upstream
//! records them at their insertion (no reachable sequence of the couplers
//! does this: one insertion batch per forward).
//!
//! [`CohesiveContactSolverCallback`] (CPL-004) is upstream's stand-alone
//! callback with the same logic, as a thin adapter over
//! [`SolidContactCallback`] (differences: every matrix is symmetric, no
//! split residual, the contact's solve-step hooks are not called, and the
//! solid stiffness is assembled with `need_to_reassemble = false` under
//! the UD-622 compat flag). Stand-alone users forward inserted cohesive
//! elements with a [`CohesiveForwarder`].
//!
//! # Upstream defects of the static / implicit couplings (D-003)
//!
//! * **UD-772** (category B): upstream's cohesive couplings build the
//!   contact model with its **own** DOF manager, so in static / implicit
//!   runs the contact stiffness goes into a matrix that is never solved
//!   (nor zeroed) and the solved `K` is the solid stiffness alone (oracle
//!   case `cohesive_static`: coupler `K` = solid `K` bitwise; upstream's
//!   Newton then needs 13 iterations for the first increment and fails at
//!   the fifth). Default: the contact stiffness is in the solved `K` (the
//!   same run converges in 12, then 2 iterations per step);
//!   [`SolidContactCompat::contact_stiffness_in_own_dof_manager`]
//!   reproduces upstream. [`CouplerSolidContact`] shares one DOF manager
//!   upstream: not affected.
//! * **UD-374** (category B): upstream searches with `savePreviousState`
//!   in every static / implicit `corrector()`, so the friction history is
//!   the previous Newton iterate. Default: committed once per converged
//!   step; [`SolidContactCompat::per_iteration_friction_history`]
//!   reproduces upstream.
//! * **UD-1291** (category B, D-109): upstream searches the contact in the
//!   static / implicit `corrector()` only, and Newton with the `residual`
//!   criterion tests the predictor's residual before any corrector, so a
//!   step whose first residual converges keeps the previous step's contact
//!   state (two bodies brought into contact by prescribed displacements
//!   converge with a penetration and no contact force), and every step's
//!   first residual is assembled on the previous step's contact set.
//!   Default: the predictor leaves a search owed, which runs on the
//!   predicted positions at the first assembly of the step (the residual,
//!   or `K` when a solver assembles it first: the time-step solver's
//!   integration-scheme predictor runs after the callback's `predictor`),
//!   without saving the friction history (the history of the step is the
//!   one upstream's order carries into it: committed once per converged
//!   step under the UD-374 default, advanced at each corrector only under
//!   its switch). [`SolidContactCompat::search_in_corrector_only`]
//!   reproduces upstream. The step sequences (Newton–Raphson, `residual`
//!   criterion):
//!
//!   | | upstream (switch on) | port default |
//!   |---|---|---|
//!   | 1 | `before_solve_step`, `predictor` (solid, then the scheme's) | the same; the callback's `predictor` leaves a search owed |
//!   | 2 | `assemble_residual` on the previous step's contact set | the owed search on the predicted positions (history kept), then `assemble_residual` |
//!   | 3 | iteration-0 test: converged → the step ends on the stale contact state | iteration-0 test on the configuration's own contact state |
//!   | 4 | loop: `K` (solid + contact), solve, `corrector` (search), `assemble_residual`, test | the same |
//!
//!   The `solution` criterion is unchanged: it declares convergence on
//!   the norm of the last increment, so a contact set changed by the last
//!   corrector's search is never tested (UD-1293, documented). The
//!   couplers are serial in both codes (the contact model refuses a
//!   distributed mesh, CONT-019): nothing changes on a distributed mesh.
//! * A bitwise replay of upstream's static cohesive coupling also needs
//!   the cohesive model's **UD-034** compat
//!   (`SolidMechanicsModelCohesive::set_symmetry_policy(
//!   SymmetryPolicy::UpstreamUpperSelection)` before `init_full`: upstream
//!   keeps `K` symmetric and drops the lower triangle of the unsymmetric
//!   cohesive tangent); with the default promotion of `K` the replay departs
//!   from upstream at Newton iteration 2.

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_cohesive::model::{CohesiveCallback, compute_cohesive_tractions};
use akantu_contact::{ContactDetector, ContactMechanicsModel};
use akantu_core::array_shared::ArrayRef;
use akantu_core::enums::{AnalysisMethod, MatrixType, ModelType, TimeStepSolverType};
use akantu_core::{Element, ElementKind, ElementType, Error, GhostType, Real, Result, TypeFilter};
use akantu_mesh::{MeshPart, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{DirichletFunctor, NeumannFunctor};
use akantu_model::model::{ModelBase, init_new_solver};
use akantu_parser::{Parser, ParserType};
use akantu_solid::SolidMechanicsModel;
use akantu_solid::model::{DOF_ID, SolidMechanicsPhysics};
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolverCallback};

/// Default id of [`CouplerSolidContact`] (upstream `"coupler_solid_contact"`).
pub const DEFAULT_ID: &str = "coupler_solid_contact";

/// Default id of [`CouplerSolidCohesiveContact`] (upstream constructor
/// default, also `"coupler_solid_contact"`; its dumper is named
/// `coupler_solid_cohesive_contact`).
pub const COHESIVE_DEFAULT_ID: &str = "coupler_solid_contact";

/// Id of the solid component of [`CouplerSolidContact`].
pub const SOLID_ID: &str = "solid_mechanics_model";

/// Id of the solid component of [`CouplerSolidCohesiveContact`].
pub const COHESIVE_SOLID_ID: &str = "solid_mechanics_model_cohesive";

/// Id of the contact component.
pub const CONTACT_ID: &str = "contact_mechanics_model";

/// The solid side of a monolithic solid–contact coupling: the solid's
/// solver callback plus the entry points the coupler calls directly.
pub trait SolidPart: SolverCallback {
    /// The solid physics.
    fn physics(&self) -> &SolidMechanicsPhysics;

    /// The solid physics, mutable.
    fn physics_mut(&mut self) -> &mut SolidMechanicsPhysics;

    /// Upstream `solid->assembleInternalForces()` (the cohesive model
    /// computes its tractions first).
    ///
    /// # Errors
    /// Assembly errors.
    fn assemble_solid_internal_forces(&mut self, dofs: &mut DofManager) -> Result<()>;

    /// Upstream `solid->assembleStiffnessMatrix(need_to_reassemble)`.
    ///
    /// # Errors
    /// Assembly errors.
    fn assemble_solid_stiffness(
        &mut self,
        dofs: &mut DofManager,
        need_to_reassemble: bool,
    ) -> Result<()>;
}

impl SolidPart for SolidMechanicsPhysics {
    fn physics(&self) -> &SolidMechanicsPhysics {
        self
    }

    fn physics_mut(&mut self) -> &mut SolidMechanicsPhysics {
        self
    }

    fn assemble_solid_internal_forces(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.assemble_internal_forces(dofs)
    }

    fn assemble_solid_stiffness(
        &mut self,
        dofs: &mut DofManager,
        need_to_reassemble: bool,
    ) -> Result<()> {
        self.assemble_stiffness_matrix(dofs, need_to_reassemble)
    }
}

impl SolidPart for CohesiveCallback<'_> {
    fn physics(&self) -> &SolidMechanicsPhysics {
        self.physics
    }

    fn physics_mut(&mut self) -> &mut SolidMechanicsPhysics {
        self.physics
    }

    /// Upstream `SolidMechanicsModelCohesive::assembleInternalForces`:
    /// cohesive tractions, then the solid model's internal forces.
    fn assemble_solid_internal_forces(&mut self, dofs: &mut DofManager) -> Result<()> {
        compute_cohesive_tractions(self.physics, dofs)?;
        self.physics.assemble_internal_forces(dofs)
    }

    /// The solid stiffness on the current positions (the cohesive laws'
    /// normals, as [`CohesiveCallback`]'s `K`).
    fn assemble_solid_stiffness(
        &mut self,
        dofs: &mut DofManager,
        need_to_reassemble: bool,
    ) -> Result<()> {
        self.physics.refresh_current_position(dofs)?;
        self.physics
            .assemble_stiffness_matrix(dofs, need_to_reassemble)
    }
}

/// Which upstream class a [`SolidContactCallback`] reproduces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackFlavour {
    /// `CouplerSolidContactTemplate` (CPL-001…003).
    Coupler,
    /// `CohesiveContactSolverCallback` (CPL-004); `upstream_stiffness`
    /// reproduces its `solid.assembleStiffnessMatrix()` with the default
    /// `need_to_reassemble = false` (UD-622).
    StandAlone {
        /// Upstream's `need_to_reassemble = false` (compat, UD-622).
        upstream_stiffness: bool,
    },
}

/// Upstream-compatibility switches of the solid–contact callbacks (D-003
/// category B: every switch `false` is the corrected default, `true`
/// reproduces upstream). Only static / implicit dynamic couplings are
/// affected (the explicit path searches once per step, in
/// `assemble_residual`, as upstream).
///
/// [`SolidContactCompat::upstream`] reproduces upstream's
/// `CouplerSolidContact`, [`SolidContactCompat::all`] upstream's
/// cohesive–contact couplings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SolidContactCompat {
    /// **UD-374.** Upstream searches with `savePreviousState` in every
    /// `corrector()`, so the friction history (`previous_*`) is the previous
    /// Newton **iterate**: the friction residual is redefined at every
    /// iteration and a sliding step does not converge. Corrected default
    /// (`false`): the correctors search without saving the history
    /// ([`ContactMechanicsModel::search_keeping_history`]) and the history is
    /// committed once per step, in `after_solve_step(converged = true)`
    /// (after the solid's hook, before the contact's): the fields then hold
    /// the converged search and tractions, and a failed step is retried from
    /// the last committed history. The first iterate of a step that follows
    /// a converged step is identical in both modes (upstream's first search
    /// saves the same state). `true`: upstream's per-iterate history.
    pub per_iteration_friction_history: bool,
    /// **UD-772**. Upstream's cohesive–contact coupler
    /// (`CouplerSolidContactTemplate<SolidMechanicsModelCohesive>`) and the
    /// stand-alone `CohesiveContactSolverCallback` build the contact model
    /// without the solid's DOF manager, so its `assembleStiffnessMatrix`
    /// goes into the contact model's **own** DOF manager, a matrix that is
    /// never solved (nor zeroed): the solved `K` of a static / implicit run
    /// is the solid stiffness alone. Corrected default (`false`): the contact
    /// stiffness is added to the solved `K`, as in [`CouplerSolidContact`].
    /// `true`: upstream's effective `K` (the contact stiffness is not
    /// assembled at all: the port's contact assembly has no side effect, and
    /// the never-solved matrix is not observable through the coupler). Not
    /// an upstream behaviour of [`CouplerSolidContact`], whose contact model
    /// shares the solid's DOF manager (refused there).
    pub contact_stiffness_in_own_dof_manager: bool,
    /// **UD-1291** (D-109). Upstream's static / implicit couplers search
    /// the contact in `corrector()` only (`coupler_solid_contact_tmpl.hh:
    /// 133-145`, `cohesive_contact_solvercallback.cc:106-118`), and Newton
    /// with the `residual` criterion tests the residual of the predictor
    /// before any corrector (`non_linear_solver_newton_raphson.cc:
    /// 130-135`): a step whose first residual already converges keeps the
    /// contact elements, gaps, normals, projections and nodal areas of the
    /// previous step's last search (e.g. two bodies brought into contact
    /// by prescribed displacements converge with a penetration and no
    /// contact force), and every first residual of a step is assembled on
    /// that stale contact set. Corrected default (`false`): the predictor
    /// leaves a search owed, which runs on the predicted positions at the
    /// first assembly of the step that follows it (the residual, or `K`
    /// when a solver assembles it first): the integration scheme's
    /// predictor runs after the callback's `predictor` in both codes, so
    /// the search cannot run in the callback's `predictor` itself. That
    /// search never saves the friction history (as the correctors' under
    /// the UD-374 default; with
    /// [`SolidContactCompat::per_iteration_friction_history`] the history
    /// still advances at each corrector only), so the history of the step
    /// is the one upstream's order carries into it. `true`: upstream's
    /// order (search in the corrector only).
    pub search_in_corrector_only: bool,
}

impl SolidContactCompat {
    /// Every switch that reproduces an upstream behaviour of
    /// [`CouplerSolidContact`] (UD-374, UD-1291; not UD-772, which concerns
    /// the cohesive–contact couplings only).
    #[must_use]
    pub fn upstream() -> Self {
        Self {
            per_iteration_friction_history: true,
            contact_stiffness_in_own_dof_manager: false,
            search_in_corrector_only: true,
        }
    }

    /// Every switch on (upstream's cohesive–contact behaviour: UD-374,
    /// UD-772, UD-1291).
    #[must_use]
    pub fn all() -> Self {
        Self {
            per_iteration_friction_history: true,
            contact_stiffness_in_own_dof_manager: true,
            search_in_corrector_only: true,
        }
    }
}

/// Combined solver callback of the monolithic solid–contact couplings.
pub struct SolidContactCallback<'a> {
    /// The solid part.
    pub solid: &'a mut dyn SolidPart,
    /// The contact part.
    pub contact: &'a mut ContactMechanicsModel,
    /// Analysis method of the coupling (upstream coupler `method`).
    pub method: AnalysisMethod,
    /// Upstream class reproduced.
    pub flavour: CallbackFlavour,
    /// Upstream-compatibility switches (default: corrected behaviour).
    pub compat: SolidContactCompat,
    /// UD-1291 corrected default: set by `predictor` (static / implicit
    /// dynamic, without [`SolidContactCompat::search_in_corrector_only`]),
    /// cleared by the search at the next assembly of the residual or of
    /// `K`, or by a corrector.
    search_owed: bool,
}

impl std::fmt::Debug for SolidContactCallback<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SolidContactCallback")
            .field("method", &self.method)
            .field("flavour", &self.flavour)
            .field("compat", &self.compat)
            .field("search_owed", &self.search_owed)
            .finish_non_exhaustive()
    }
}

impl<'a> SolidContactCallback<'a> {
    /// The callback of the upstream class `flavour` with the switches
    /// `compat`.
    pub fn new(
        solid: &'a mut dyn SolidPart,
        contact: &'a mut ContactMechanicsModel,
        method: AnalysisMethod,
        flavour: CallbackFlavour,
        compat: SolidContactCompat,
    ) -> Self {
        Self {
            solid,
            contact,
            method,
            flavour,
            compat,
            search_owed: false,
        }
    }

    /// The coupler's callback (CPL-001…003) with the corrected defaults.
    pub fn coupler(
        solid: &'a mut dyn SolidPart,
        contact: &'a mut ContactMechanicsModel,
        method: AnalysisMethod,
    ) -> Self {
        Self::new(
            solid,
            contact,
            method,
            CallbackFlavour::Coupler,
            SolidContactCompat::default(),
        )
    }

    /// The same callback with the upstream-compatibility switches `compat`.
    #[must_use]
    pub fn with_compat(mut self, compat: SolidContactCompat) -> Self {
        self.compat = compat;
        self
    }

    /// Positions of the detector ← current positions of the solid, then
    /// `contact.search()` (upstream
    /// `current_positions.copy(solid->getCurrentPosition()); contact->search();`).
    /// `save_history = false` searches without saving the friction history
    /// (UD-374 corrected default inside the Newton loop).
    fn update_positions_and_search(&mut self, dofs: &DofManager, save_history: bool) -> Result<()> {
        let cp = self.solid.physics_mut().refresh_current_position(dofs)?;
        self.contact.positions_mut().copy_from(&cp)?;
        if save_history {
            self.contact.search()
        } else {
            self.contact.search_keeping_history()
        }
    }

    fn is_static_or_implicit(&self) -> bool {
        matches!(
            self.method,
            AnalysisMethod::Static | AnalysisMethod::ImplicitDynamic
        )
    }

    /// UD-1291 corrected default: the search owed since the predictor, on
    /// the current (predicted) positions and without saving the friction
    /// history, before the first assembly of the step.
    fn search_if_owed(&mut self, dofs: &DofManager) -> Result<()> {
        if std::mem::take(&mut self.search_owed) {
            self.update_positions_and_search(dofs, false)?;
        }
        Ok(())
    }
}

impl SolverCallback for SolidContactCallback<'_> {
    fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        match self.flavour {
            CallbackFlavour::Coupler => match matrix_id {
                "K" | "M" => MatrixType::Symmetric,
                _ => MatrixType::NotDefined,
            },
            CallbackFlavour::StandAlone { .. } => MatrixType::Symmetric,
        }
    }

    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        match matrix_id {
            "K" => {
                // UD-1291: a solver that assembles `K` before the first
                // residual (linear Newton types) gets the predictor search
                // here
                self.search_if_owed(dofs)?;
                let need = match self.flavour {
                    CallbackFlavour::Coupler => true,
                    CallbackFlavour::StandAlone { upstream_stiffness } => !upstream_stiffness,
                };
                self.solid.assemble_solid_stiffness(dofs, need)?;
                // UD-772: upstream's cohesive couplers assemble the contact
                // stiffness into the contact model's own (never solved) DOF
                // manager
                if self.is_static_or_implicit() && !self.compat.contact_stiffness_in_own_dof_manager
                {
                    self.contact.assemble_stiffness_matrix(dofs)?;
                }
                Ok(())
            }
            "M" => self.solid.assemble_matrix(dofs, "M"),
            _ => Ok(()),
        }
    }

    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if matrix_id == "M" {
            self.solid.assemble_lumped_matrix(dofs, "M")
        } else {
            Ok(())
        }
    }

    /// Explicit lumped mass: positions ← current, `contact.search()` (as
    /// upstream); static / implicit dynamic: the search owed since the
    /// predictor (UD-1291 corrected default). Then the solid's and the
    /// contact's internal forces and `r += f_ext + f_int + f_contact`.
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        if self.method == AnalysisMethod::ExplicitLumpedMass {
            self.update_positions_and_search(dofs, true)?;
        } else {
            self.search_if_owed(dofs)?;
        }
        self.solid.assemble_solid_internal_forces(dofs)?;
        self.contact.assemble_internal_forces()?;
        let p = self.solid.physics();
        dofs.assemble_to_residual(DOF_ID, p.external_force()?.view(), 1.0)?;
        dofs.assemble_to_residual(DOF_ID, p.internal_force()?.view(), 1.0)?;
        dofs.assemble_to_residual(DOF_ID, self.contact.internal_force().view(), 1.0)
    }

    fn assemble_residual_part(&mut self, dofs: &mut DofManager, part: &str) -> Result<()> {
        if self.flavour != CallbackFlavour::Coupler {
            return Ok(());
        }
        let p = self.solid.physics();
        match part {
            "external" => {
                dofs.assemble_to_residual(DOF_ID, p.external_force()?.view(), 1.0)?;
                dofs.assemble_to_residual(DOF_ID, self.contact.internal_force().view(), 1.0)
            }
            "internal" => dofs.assemble_to_residual(DOF_ID, p.internal_force()?.view(), 1.0),
            other => Err(Error::invalid(format!(
                "The residual part {other} is unknown by the solver callback"
            ))),
        }
    }

    fn can_split_residual(&self) -> bool {
        self.flavour == CallbackFlavour::Coupler
    }

    /// The solid's predictor; for static / implicit dynamic without
    /// [`SolidContactCompat::search_in_corrector_only`] (UD-1291 corrected
    /// default) a contact search is owed: it runs at the next assembly of
    /// the residual or of `K`, after the time-step solver's integration
    /// scheme has predicted the positions.
    fn predictor(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.solid.predictor(dofs)?;
        self.search_owed = self.is_static_or_implicit() && !self.compat.search_in_corrector_only;
        Ok(())
    }

    /// The solid's corrector, then for static / implicit dynamic the
    /// contact search on the current positions: with the friction history
    /// saved at every iterate under [`SolidContactCompat::
    /// per_iteration_friction_history`] (upstream, UD-374), without saving it
    /// otherwise (it is committed by a converged `after_solve_step`).
    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.solid.corrector(dofs)?;
        // the corrector's search replaces a predictor search still owed
        self.search_owed = false;
        if self.is_static_or_implicit() {
            let save = self.compat.per_iteration_friction_history;
            self.update_positions_and_search(dofs, save)?;
        }
        Ok(())
    }

    fn before_solve_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.solid.before_solve_step(dofs)?;
        if self.flavour == CallbackFlavour::Coupler {
            self.contact.before_solve_step()?;
        }
        Ok(())
    }

    /// The solid's hook; for static / implicit dynamic with the corrected
    /// UD-374 default, a converged step commits the contact's friction
    /// history (the converged search and tractions: the history of the next
    /// step); then the contact's hook (coupler flavour only, as upstream).
    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        self.solid.after_solve_step(dofs, converged)?;
        if converged && self.is_static_or_implicit() && !self.compat.per_iteration_friction_history
        {
            self.contact.save_previous_state()?;
        }
        if self.flavour == CallbackFlavour::Coupler {
            self.contact.after_solve_step(converged)?;
        }
        Ok(())
    }

    /// Upstream coupler `initSolver(type)`: the solid's, then the contact's.
    fn init_solver(&mut self, dofs: &mut DofManager, ty: TimeStepSolverType) -> Result<()> {
        if self.flavour != CallbackFlavour::Coupler {
            return Ok(());
        }
        self.solid.init_solver(dofs, ty)?;
        <ContactMechanicsModel as SolverCallback>::init_solver(self.contact, dofs, ty)
    }

    /// Upstream: delegated to the solid.
    fn default_solver_type(&self) -> TimeStepSolverType {
        self.solid.default_solver_type()
    }

    /// Upstream: delegated to the solid.
    fn default_solver_options(&self, ty: TimeStepSolverType) -> ModelSolverOptions {
        self.solid.default_solver_options(ty)
    }
}

/// The coupler's own model state (upstream `Model` members of the
/// coupler: id, type, method, input-file section).
#[derive(Debug)]
struct CouplerState {
    base: ModelBase,
}

/// Replace `solver` by the coupler's `ModelSolver`: DOF manager
/// `<id>:dof_manager_default` on the mesh's communicator (upstream
/// `initDOFManager(nullptr)` of the coupler; the solid receives it and
/// ignores its own model section).
///
/// # Errors
/// A `dof_manager_type` other than `default` in the coupler's section:
/// upstream would allocate it (`model_solver.cc:78-95`), the port's contact
/// couplers run on the default DOF manager only (the PETSc DOF manager is
/// wired for the solid and phase-field models), refused instead
/// of being ignored.
fn coupler_solver(state: &CouplerState) -> Result<ModelSolver> {
    if let Some(section) = state.base.parser_section()
        && let Ok(p) =
            section.get_parameter("dof_manager_type", akantu_parser::SearchScope::Current)
    {
        let t = p.to_word().to_string_lossy().into_owned();
        if t != "default" {
            return Err(Error::Unsupported(format!(
                "dof_manager_type = {t} in the section of the contact coupler '{}': the contact \
                 couplers run on the default DOF manager only",
                state.base.id
            )));
        }
    }
    let comm = state.base.mesh.read()?.communicator().clone();
    let dm = DofManager::new(format!("{}:dof_manager_default", state.base.id), comm);
    Ok(ModelSolver::new(dm))
}

/// Upstream `initDOFManager(section)` of the coupler: the solvers of the
/// coupler's `model <type> [ … ]` section (upstream creates them before the
/// solid and contact exist, a null dereference when the section has any
/// `time_step_solver`; the port creates them once both parts exist, UD-621).
fn init_coupler_section(
    state: &CouplerState,
    solver: &mut ModelSolver,
    cb: &mut SolidContactCallback<'_>,
) -> Result<()> {
    if let Some(section) = state.base.parser_section().cloned()
        && section
            .sub_sections_of(ParserType::TimeStepSolver)
            .next()
            .is_some()
    {
        // upstream's global `Parser::isPermissive()` for the
        // `non_linear_solver` sub-section
        let permissive = state.base.parser.is_permissive();
        solver.init_from_section(&section, cb, permissive)?;
    }
    Ok(())
}

/// Monolithic solid–contact coupler (upstream `CouplerSolidContact` =
/// `CouplerSolidContactTemplate<SolidMechanicsModel>`, CPL-001/002).
#[derive(Debug)]
pub struct CouplerSolidContact {
    state: CouplerState,
    /// The solid mechanics model (id `solid_mechanics_model`), owner of the
    /// shared `ModelSolver`.
    pub solid: SolidMechanicsModel,
    /// The contact mechanics model (id `contact_mechanics_model`) on the
    /// same mesh and DOF family.
    pub contact: ContactMechanicsModel,
    compat: SolidContactCompat,
    /// The coupler's own dumper `coupler_solid_contact` (`dump.rs`).
    pub(crate) dumpable: akantu_io::dumper::Dumpable,
}

impl CouplerSolidContact {
    /// Upstream `CouplerSolidContact(mesh, dim, id)` with the input-file
    /// snapshot `parser` (the solid reads its materials from it, the contact
    /// model its detector and resolutions, the coupler its own solvers).
    ///
    /// # Errors
    /// Construction errors of the parts (dimension, mesh domain, no
    /// `contact_detector` section, invalid sections).
    pub fn with_parser(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        let mut base = ModelBase::new(
            mesh.clone(),
            ModelType::CouplerSolidContact,
            spatial_dimension,
            id,
        )?;
        base.parser = parser.clone();
        let dim = base.spatial_dimension;
        // upstream registers the coupler's dumper first (then the parts
        // theirs, each the new mesh default; `dump.rs`)
        let dumpable = crate::dump::coupler_dumpable(
            crate::dump::SOLID_CONTACT_DUMPER,
            id,
            dim,
            ElementKind::Regular,
        )?;
        let state = CouplerState { base };
        let mut solid =
            SolidMechanicsModel::with_parser(mesh.clone(), Some(dim), SOLID_ID, parser.clone())?;
        solid.solver = coupler_solver(&state)?;
        let mut contact = ContactMechanicsModel::with_parser(mesh, Some(dim), CONTACT_ID, parser)?;
        {
            let SolidMechanicsModel {
                physics, solver, ..
            } = &mut solid;
            let mut cb = SolidContactCallback::coupler(physics, &mut contact, state.base.method);
            init_coupler_section(&state, solver, &mut cb)?;
        }
        Ok(Self {
            state,
            solid,
            contact,
            compat: SolidContactCompat::default(),
            dumpable,
        })
    }

    /// Coupler id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.state.base.id
    }

    /// Upstream-compatibility switches of the coupler (default: corrected).
    #[must_use]
    pub fn upstream_compat(&self) -> SolidContactCompat {
        self.compat
    }

    /// Set the upstream-compatibility switches:
    /// [`SolidContactCompat::per_iteration_friction_history`] (UD-374) and
    /// [`SolidContactCompat::search_in_corrector_only`] (UD-1291) reproduce
    /// upstream behaviours of this coupler ([`SolidContactCompat::upstream`]
    /// sets both).
    ///
    /// # Errors
    /// [`SolidContactCompat::contact_stiffness_in_own_dof_manager`] set:
    /// upstream's `CouplerSolidContact` shares one DOF manager between the
    /// solid and the contact model (the contact stiffness is in the solved
    /// `K`), so there is nothing to reproduce (UD-772 concerns the
    /// cohesive–contact couplings only).
    pub fn set_upstream_compat(&mut self, compat: SolidContactCompat) -> Result<()> {
        if compat.contact_stiffness_in_own_dof_manager {
            return Err(Error::invalid(
                "CouplerSolidContact: contact_stiffness_in_own_dof_manager (UD-772) is not an \
                 upstream behaviour of this coupler (its contact model shares the solid's DOF \
                 manager)",
            ));
        }
        self.compat = compat;
        Ok(())
    }

    /// Analysis method of the last [`CouplerSolidContact::init_full`]
    /// (upstream coupler `method`, default `_explicit_lumped_mass`).
    #[must_use]
    pub fn analysis_method(&self) -> AnalysisMethod {
        self.state.base.method
    }

    /// Upstream `getContactMechanicsModel().getContactDetector()
    /// .setSurfaceSelector(selector)` with the mesh group
    /// `contact_surface` that upstream's selector constructor creates:
    /// the selector is set on the contact model and its surface mirrored
    /// as the mesh group through a group-only mutation of the shared mesh
    /// (the solid model is its consumer,
    /// `akantu_contact::selector`).
    ///
    /// # Errors
    /// A selector of another mesh part; the mesh mutation's errors.
    pub fn set_surface_selector(
        &mut self,
        selector: Box<dyn akantu_contact::SurfaceSelector>,
    ) -> Result<()> {
        self.contact.set_surface_selector(selector)?;
        let (token, mut parts) = self.solid.split_for_mutation();
        self.contact
            .write_contact_surface_group(&mut [(token, &mut parts)])
    }

    /// Upstream `initFull(options)`: the coupler's default solver for the
    /// method if none exists (default id, type and options of the solid,
    /// `initSolver` of both parts), then `solid.initFull(method)` and
    /// `contact.initFull(method)`.
    ///
    /// # Errors
    /// Solver creation, material or resolution errors.
    pub fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        let method = options.analysis_method;
        self.state.base.method = method;
        let SolidMechanicsModel {
            physics, solver, ..
        } = &mut self.solid;
        if !solver.has_default_solver() {
            let id = physics.default_solver_id(method);
            let mut cb = SolidContactCallback::coupler(physics, &mut self.contact, method)
                .with_compat(self.compat);
            init_new_solver(solver, &mut cb, id, options.solver_options.clone())?;
        }
        self.solid.init_full(&ModelOptions::new(method))?;
        self.contact.init_full(method)
    }

    /// Upstream `solveStep(solver_id)` of the coupler: the shared solver
    /// drives the combined callback.
    ///
    /// # Errors
    /// Stale mesh revision, unknown solver, solver and callback errors.
    pub fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        let method = self.state.base.method;
        let compat = self.compat;
        let contact = &mut self.contact;
        self.solid
            .solve_step_with(solver_id, |physics, solver, id| {
                let mut cb =
                    SolidContactCallback::coupler(physics, contact, method).with_compat(compat);
                solver.solve_step(&mut cb, id)
            })
    }

    /// Upstream `assembleInternalForces()` of the coupler (outside a solve):
    /// solid then contact internal forces.
    ///
    /// # Errors
    /// Assembly errors.
    pub fn assemble_internal_forces(&mut self) -> Result<()> {
        self.solid.assemble_internal_forces()?;
        self.contact.assemble_internal_forces()
    }

    /// Upstream `assembleStiffnessMatrix()` of the coupler (outside a
    /// solve): `solid.assembleStiffnessMatrix(true)`, plus the contact
    /// stiffness for static / implicit dynamic methods.
    ///
    /// # Errors
    /// Assembly errors.
    pub fn assemble_stiffness_matrix(&mut self) -> Result<()> {
        let method = self.state.base.method;
        let SolidMechanicsModel {
            physics, solver, ..
        } = &mut self.solid;
        let contact = &mut self.contact;
        solver
            .dof_manager
            .collective("coupler:assemble_stiffness", |dm| {
                physics.assemble_stiffness_matrix(dm, true)?;
                if matches!(
                    method,
                    AnalysisMethod::Static | AnalysisMethod::ImplicitDynamic
                ) {
                    contact.assemble_stiffness_matrix(dm)?;
                }
                dm.finalize_matrix("K")
            })
    }

    /// Upstream `setTimeStep(dt, solver_id)` of the coupler (shared
    /// solver): `ModelSolver::setTimeStep`, which the coupler does not
    /// override, so no dumper gets a time step (no `.pvd`, `dump.rs`).
    ///
    /// # Errors
    /// Unknown solver.
    pub fn set_time_step(&mut self, time_step: Real, solver_id: Option<&str>) -> Result<()> {
        self.solid.set_solver_time_step(time_step, solver_id)
    }

    /// Upstream `getStableTimeStep()`: the solid's.
    ///
    /// # Errors
    /// See `SolidMechanicsModel::stable_time_step`.
    pub fn stable_time_step(&mut self) -> Result<Real> {
        self.solid.stable_time_step()
    }

    /// Upstream `applyBC(functor, group)`: forwarded to the solid.
    ///
    /// # Errors
    /// Unknown group, functor errors.
    pub fn apply_dirichlet_bc(
        &mut self,
        functor: &dyn DirichletFunctor,
        group: &str,
    ) -> Result<()> {
        self.solid.apply_dirichlet_bc(functor, group)
    }

    /// Upstream `applyBC(functor, group)` (Neumann): forwarded to the solid.
    ///
    /// # Errors
    /// Unknown group, functor errors.
    pub fn apply_neumann_bc(&mut self, functor: &dyn NeumannFunctor, group: &str) -> Result<()> {
        self.solid.apply_neumann_bc(functor, group)
    }

    /// Upstream `getContactForce()`: returns the **solid's internal force**
    /// (as upstream; the contact force is `contact.internal_force()`;
    /// UD-623, category C).
    ///
    /// # Errors
    /// Not allocated.
    pub fn contact_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.solid.internal_force()
    }

    /// Upstream `getContactDetector()`.
    #[must_use]
    pub fn contact_detector(&self) -> &ContactDetector {
        self.contact.contact_detector()
    }

    /// Upstream `getDisplacement()`.
    ///
    /// # Errors
    /// Not allocated.
    pub fn displacement(&self) -> Result<ArrayRef<'_, Real>> {
        self.solid.displacement()
    }

    /// Upstream `getExternalForce()`.
    ///
    /// # Errors
    /// Not allocated.
    pub fn external_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.solid.external_force()
    }

    /// Upstream `isDefaultSolverExplicit()`.
    #[must_use]
    pub fn is_default_solver_explicit(&self) -> bool {
        self.state.base.method == AnalysisMethod::ExplicitLumpedMass
    }
}

/// Cohesive elements per `(type, ghost)` already forwarded to the contact
/// model.
type CohesiveCounts = Vec<(ElementType, GhostType, usize)>;

/// Forwards the cohesive elements inserted since its last call to a
/// contact model's surface selector — what upstream's
/// `CohesiveSurfaceSelector` / `AllSurfaceSelector` receive as mesh events
/// (`onElementsAdded`, sent by `insertElements` after node doubling). The
/// port has no mesh-event dispatch to the contact model: whoever inserts
/// cohesive elements (`SolidMechanicsModelCohesive::check_cohesive_stress`,
/// intrinsic insertion in `init_full`) calls [`CohesiveForwarder::forward`]
/// before the next contact search. [`CouplerSolidCohesiveContact`] does it
/// itself; this type serves stand-alone uses of
/// [`CohesiveContactSolverCallback`].
///
/// Insertions are forwarded in batches: the new elements of a type are the
/// trailing indices of the type since the last call, read with the
/// connectivity of the forward (identical to upstream whenever at most one
/// insertion batch doubles nodes between two forwards — every reachable
/// sequence of the couplers).
#[derive(Clone, Debug, Default)]
pub struct CohesiveForwarder {
    forwarded: CohesiveCounts,
}

impl CohesiveForwarder {
    /// A forwarder for which the cohesive elements already in `mesh` count
    /// as forwarded.
    ///
    /// # Errors
    /// The mesh lock is poisoned.
    pub fn new(mesh: &SharedMesh) -> Result<Self> {
        Ok(Self {
            forwarded: cohesive_counts(mesh)?,
        })
    }

    /// Forward the cohesive elements inserted into `mesh` since the last
    /// call to `contact`'s surface selector; returns their number.
    ///
    /// # Errors
    /// Selector errors (no surface selector set, no facet mesh); nothing is
    /// recorded as forwarded on error.
    pub fn forward(
        &mut self,
        mesh: &SharedMesh,
        contact: &mut ContactMechanicsModel,
    ) -> Result<usize> {
        let now = cohesive_counts(mesh)?;
        let mut new: Vec<Element> = Vec::new();
        for &(ty, ghost, n) in &now {
            let old = self
                .forwarded
                .iter()
                .find(|(t, g, _)| *t == ty && *g == ghost)
                .map_or(0, |e| e.2);
            for i in old..n {
                new.push(Element::try_new(ty, i, ghost)?);
            }
        }
        if !new.is_empty() {
            contact.on_cohesive_elements_added(&new)?;
        }
        self.forwarded = now;
        Ok(new.len())
    }
}

/// Current number of cohesive elements per `(type, ghost)` of `mesh`.
fn cohesive_counts(mesh: &SharedMesh) -> Result<CohesiveCounts> {
    let mesh = mesh.read()?;
    let mut v = Vec::new();
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter {
            spatial_dimension: None,
            ghost_type: ghost,
            kind: Some(ElementKind::Cohesive),
        }) {
            v.push((ty, ghost, mesh.nb_element(ty, ghost)));
        }
    }
    Ok(v)
}

/// Monolithic solid–cohesive–contact coupler (upstream
/// `CouplerSolidCohesiveContact` =
/// `CouplerSolidContactTemplate<SolidMechanicsModelCohesive>`, CPL-003).
#[derive(Debug)]
pub struct CouplerSolidCohesiveContact {
    state: CouplerState,
    /// The cohesive solid model (id `solid_mechanics_model_cohesive`), owner
    /// of the shared `ModelSolver`.
    pub solid: SolidMechanicsModelCohesive,
    /// The contact mechanics model on the facet mesh (id
    /// `contact_mechanics_model`). Upstream gives it its own DOF manager,
    /// into which its stiffness is assembled and never solved (UD-772): the
    /// port adds the contact stiffness to the solved `K` by default and
    /// reproduces upstream under
    /// [`SolidContactCompat::contact_stiffness_in_own_dof_manager`].
    pub contact: ContactMechanicsModel,
    forwarder: CohesiveForwarder,
    compat: SolidContactCompat,
    /// The coupler's own dumper `coupler_solid_cohesive_contact`
    /// (`dump.rs`).
    pub(crate) dumpable: akantu_io::dumper::Dumpable,
}

impl CouplerSolidCohesiveContact {
    /// Upstream `CouplerSolidCohesiveContact(mesh, dim, id)` with the
    /// input-file snapshot `parser`: the cohesive solid model (which builds
    /// the facet mesh) and the contact model on the facet mesh.
    ///
    /// # Errors
    /// Construction errors of the parts.
    pub fn with_parser(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        let mut base = ModelBase::new(
            mesh.clone(),
            ModelType::CouplerSolidCohesiveContact,
            spatial_dimension,
            id,
        )?;
        base.parser = parser.clone();
        let dim = base.spatial_dimension;
        let dumpable = crate::dump::coupler_dumpable(
            crate::dump::SOLID_COHESIVE_CONTACT_DUMPER,
            id,
            dim,
            ElementKind::Cohesive,
        )?;
        let state = CouplerState { base };
        let mut solid = SolidMechanicsModelCohesive::with_parser(
            mesh.clone(),
            Some(dim),
            COHESIVE_SOLID_ID,
            parser.clone(),
        )?;
        solid.smm.solver = coupler_solver(&state)?;
        let mut contact =
            ContactMechanicsModel::on_part(mesh, MeshPart::Facets, Some(dim), CONTACT_ID, parser)?;
        {
            let SolidMechanicsModel {
                physics, solver, ..
            } = &mut solid.smm;
            let mut cohesive = CohesiveCallback::new(physics);
            let mut cb =
                SolidContactCallback::coupler(&mut cohesive, &mut contact, state.base.method);
            init_coupler_section(&state, solver, &mut cb)?;
        }
        let forwarder = CohesiveForwarder::new(&state.base.mesh)?;
        Ok(Self {
            state,
            solid,
            contact,
            forwarder,
            compat: SolidContactCompat::default(),
            dumpable,
        })
    }

    /// Coupler id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.state.base.id
    }

    /// Analysis method of the last [`CouplerSolidCohesiveContact::init_full`].
    #[must_use]
    pub fn analysis_method(&self) -> AnalysisMethod {
        self.state.base.method
    }

    /// Upstream-compatibility switches of the coupler (default: corrected).
    #[must_use]
    pub fn upstream_compat(&self) -> SolidContactCompat {
        self.compat
    }

    /// Set the upstream-compatibility switches (UD-374, UD-772, UD-1291;
    /// each reproduces an upstream behaviour of this coupler,
    /// [`SolidContactCompat::all`] sets every one).
    pub fn set_upstream_compat(&mut self, compat: SolidContactCompat) {
        self.compat = compat;
    }

    /// Upstream `initFull(_analysis_method, _is_extrinsic)`
    /// (`CouplerSolidCohesiveContactOptions`): the coupler's default solver
    /// for the method if none exists, then
    /// `solid.initFull(method, is_extrinsic)` and `contact.initFull(method)`.
    ///
    /// # Errors
    /// Solver creation, material, insertion or resolution errors.
    pub fn init_full(&mut self, options: &ModelOptions, is_extrinsic: bool) -> Result<()> {
        let method = options.analysis_method;
        self.state.base.method = method;
        let SolidMechanicsModel {
            physics, solver, ..
        } = &mut self.solid.smm;
        if !solver.has_default_solver() {
            let id = physics.default_solver_id(method);
            let mut cohesive = CohesiveCallback::new(physics);
            let mut cb = SolidContactCallback::coupler(&mut cohesive, &mut self.contact, method)
                .with_compat(self.compat);
            init_new_solver(solver, &mut cb, id, options.solver_options.clone())?;
        }
        self.solid
            .init_full(&ModelOptions::new(method), is_extrinsic)?;
        self.contact.init_full(method)?;
        self.forward_new_cohesive_elements()
    }

    /// Forward the cohesive elements inserted since the last call to the
    /// contact model's surface selector (upstream: the selector's
    /// `onElementsAdded` mesh-event handler). Called by
    /// [`CouplerSolidCohesiveContact::solve_step`] and after insertions
    /// through the coupler; call it after inserting through `solid`
    /// directly if the contact surface must be read before the next solve.
    ///
    /// # Errors
    /// Selector errors (no surface selector set: nothing is forwarded and
    /// no error is raised while no cohesive element exists).
    pub fn forward_new_cohesive_elements(&mut self) -> Result<()> {
        let n = self
            .forwarder
            .forward(&self.state.base.mesh, &mut self.contact)?;
        if n > 0 {
            // the facet-mesh group `contact_surface` follows the selector's
            // surface (upstream: the cohesive selectors' `onElementsAdded`
            // add the facets to it)
            self.write_contact_surface_group()?;
        }
        Ok(())
    }

    /// Upstream `getContactMechanicsModel().getContactDetector()
    /// .setSurfaceSelector(selector)` with the facet-mesh group
    /// `contact_surface` that upstream's cohesive selectors create (empty
    /// until the first insertion; the physical facets for the
    /// `AllSurfaceSelector`): the selector is set on the contact model and
    /// its surface mirrored as the group through a group-only mutation of
    /// the shared mesh; [`CouplerSolidCohesiveContact::forward_new_cohesive_elements`]
    /// rewrites it after each insertion.
    ///
    /// # Errors
    /// A selector of another mesh part; the mesh mutation's errors.
    pub fn set_surface_selector(
        &mut self,
        selector: Box<dyn akantu_contact::SurfaceSelector>,
    ) -> Result<()> {
        self.contact.set_surface_selector(selector)?;
        self.write_contact_surface_group()
    }

    /// The selector's surface as the mesh group `contact_surface` (the
    /// cohesive solid model is the mesh's consumer).
    fn write_contact_surface_group(&mut self) -> Result<()> {
        let (token, mut parts) = self.solid.smm.split_for_mutation();
        self.contact
            .write_contact_surface_group(&mut [(token, &mut parts)])
    }

    /// Upstream `checkCohesiveStress()` (forwarded to the solid), then the
    /// new cohesive elements to the contact surface. Returns the number of
    /// inserted elements.
    ///
    /// # Errors
    /// Insertion and selector errors.
    pub fn check_cohesive_stress(&mut self) -> Result<usize> {
        let n = self.solid.check_cohesive_stress()?;
        self.forward_new_cohesive_elements()?;
        Ok(n)
    }

    /// Upstream `solveStep(solver_id)` of the coupler.
    ///
    /// # Errors
    /// Stale mesh revision, unknown solver, solver and callback errors.
    pub fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        self.forward_new_cohesive_elements()?;
        let method = self.state.base.method;
        let compat = self.compat;
        let contact = &mut self.contact;
        self.solid
            .smm
            .solve_step_with(solver_id, |physics, solver, id| {
                let mut cohesive = CohesiveCallback::new(physics);
                let mut cb = SolidContactCallback::coupler(&mut cohesive, contact, method)
                    .with_compat(compat);
                solver.solve_step(&mut cb, id)
            })
    }

    /// Upstream `setTimeStep(dt, solver_id)`: `ModelSolver::setTimeStep`
    /// (the coupler does not override it), so neither the solid's default
    /// dumper nor its `cohesive elements` dumper gets a time step (no
    /// `.pvd`, `dump.rs`).
    ///
    /// # Errors
    /// Unknown solver.
    pub fn set_time_step(&mut self, time_step: Real, solver_id: Option<&str>) -> Result<()> {
        self.solid.smm.set_solver_time_step(time_step, solver_id)
    }

    /// Upstream `getStableTimeStep()`: the solid's.
    ///
    /// # Errors
    /// See `SolidMechanicsModel::stable_time_step`.
    pub fn stable_time_step(&mut self) -> Result<Real> {
        self.solid.stable_time_step()
    }

    /// Upstream `applyBC(functor, group)`: forwarded to the solid.
    ///
    /// # Errors
    /// Unknown group, functor errors.
    pub fn apply_dirichlet_bc(
        &mut self,
        functor: &dyn DirichletFunctor,
        group: &str,
    ) -> Result<()> {
        self.solid.smm.apply_dirichlet_bc(functor, group)
    }

    /// Upstream `getContactForce()`: the solid's internal force (UD-623).
    ///
    /// # Errors
    /// Not allocated.
    pub fn contact_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.solid.smm.internal_force()
    }
}

/// Upstream `CohesiveContactSolverCallback(solid, contact, method)`
/// (CPL-004): a stand-alone callback for `solid.solveStep(callback)`.
/// Construct it inside [`SolidMechanicsModel::solve_step_with`] of the
/// cohesive model's solid (the callback borrows the physics); forward the
/// cohesive elements inserted between solves with a [`CohesiveForwarder`]:
///
/// ```no_run
/// use akantu_cohesive::SolidMechanicsModelCohesive;
/// use akantu_cohesive::model::CohesiveCallback;
/// use akantu_contact::ContactMechanicsModel;
/// use akantu_core::enums::AnalysisMethod;
/// use akantu_couplers::{CohesiveContactSolverCallback, CohesiveForwarder};
/// use akantu_mesh::SharedMesh;
///
/// fn step(
///     mesh: &SharedMesh,
///     solid: &mut SolidMechanicsModelCohesive,
///     contact: &mut ContactMechanicsModel,
///     forwarder: &mut CohesiveForwarder,
/// ) -> akantu_core::Result<()> {
///     solid.check_cohesive_stress()?;
///     forwarder.forward(mesh, contact)?;
///     let method = AnalysisMethod::ExplicitLumpedMass;
///     solid.smm.solve_step_with(None, |physics, solver, id| {
///         let mut cohesive = CohesiveCallback::new(physics);
///         let mut cb = CohesiveContactSolverCallback::new(&mut cohesive, contact, method);
///         solver.solve_step(cb.as_callback(), id)
///     })
/// }
/// ```
#[derive(Debug)]
pub struct CohesiveContactSolverCallback<'a> {
    inner: SolidContactCallback<'a>,
}

impl<'a> CohesiveContactSolverCallback<'a> {
    /// The callback with the corrected defaults (UD-374, UD-772, UD-622,
    /// UD-1291).
    pub fn new(
        solid: &'a mut CohesiveCallback<'_>,
        contact: &'a mut ContactMechanicsModel,
        method: AnalysisMethod,
    ) -> Self {
        Self::with_compat(solid, contact, method, SolidContactCompat::default(), false)
    }

    /// The callback with the upstream-compatibility switches `compat`
    /// (UD-374, UD-772, UD-1291) and `upstream_stiffness`, which reproduces
    /// upstream's `solid.assembleStiffnessMatrix()` with the default
    /// `need_to_reassemble = false` (UD-622: latent upstream, every cohesive
    /// law reports a changed stiffness; with a solid part whose materials
    /// report an unchanged stiffness `K` would not be zeroed and the contact
    /// moduli would accumulate over Newton iterations).
    pub fn with_compat(
        solid: &'a mut CohesiveCallback<'_>,
        contact: &'a mut ContactMechanicsModel,
        method: AnalysisMethod,
        compat: SolidContactCompat,
        upstream_stiffness: bool,
    ) -> Self {
        Self {
            inner: SolidContactCallback::new(
                solid,
                contact,
                method,
                CallbackFlavour::StandAlone { upstream_stiffness },
                compat,
            ),
        }
    }

    /// The callback to hand to a solver.
    pub fn as_callback(&mut self) -> &mut SolidContactCallback<'a> {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::SolidContactCompat;

    /// The compat sets name every switch: the destructuring lists every
    /// field (a new switch does not compile here until it is placed), every
    /// one is on in `all()`, and `upstream()` (the upstream behaviours of
    /// `CouplerSolidContact`) is `all()` without UD-772, which concerns the
    /// cohesive–contact couplings only.
    #[test]
    fn compat_sets_name_every_switch() {
        let SolidContactCompat {
            per_iteration_friction_history,
            contact_stiffness_in_own_dof_manager,
            search_in_corrector_only,
        } = SolidContactCompat::all();
        assert!(per_iteration_friction_history);
        assert!(contact_stiffness_in_own_dof_manager);
        assert!(search_in_corrector_only);
        assert_eq!(
            SolidContactCompat::upstream(),
            SolidContactCompat {
                contact_stiffness_in_own_dof_manager: false,
                ..SolidContactCompat::all()
            }
        );
        let SolidContactCompat {
            per_iteration_friction_history,
            contact_stiffness_in_own_dof_manager,
            search_in_corrector_only,
        } = SolidContactCompat::default();
        assert!(
            !per_iteration_friction_history
                && !contact_stiffness_in_own_dof_manager
                && !search_in_corrector_only,
            "the default is the corrected behaviour"
        );
    }
}
