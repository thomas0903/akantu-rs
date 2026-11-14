// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/ntn_contact/{ntn_base_friction.hh, ntn_base_friction.cc,
// ntn_friction.hh, ntn_friction_tmpl.hh, ntrf_friction.hh,
// ntrf_friction_tmpl.hh}, friction_regularisations/* and friction_laws/*.

//! Friction of the traction-at-split-node contact (upstream
//! `NTNBaseFriction`, `NTNFriction<Law, Reg>`, `NTRFFriction<Law, Reg>`;
//! TASN-004, TASN-012…037).
//!
//! One struct, [`NtnFriction`], holds the per-contact-node arrays of the
//! upstream base class, the regularisation and law parameters (all per-node
//! [`SynchronizedArray`]s set uniformly from the input file, upstream
//! `_pat_parsmod`), and dispatches on [`FrictionLaw`] × [`Regularisation`]
//! in the order of the upstream template stack:
//!
//! * `computeFrictionTraction`: stick traction (the tangential traction that
//!   stops the relative tangential motion, `Z/2 (v_t + Δt a_t,new −
//!   ½ Δt a_t,old)`), frictional strength (law → Coulomb `μ p` →
//!   regularisation), then a node in contact slides (traction scaled to the
//!   strength) iff its stick traction exceeds the strength.
//! * `updateSlip`: slip, cumulative slip and slip velocity from the relative
//!   tangential displacement increment (reset on sticking nodes).
//!
//! The friction does not keep a reference to its contact: every operation
//! takes the contact (and the model state). The friction arrays follow the
//! contact's node list through the contact's [`crate::sync_array::SyncJournal`]
//! (upstream: the arrays are registered as dependents of `slaves`).
//!
//! **UD-039:** see [`crate::factory`] for the regularisation selector and
//! [`crate::TasnUpstreamCompat::rate_and_state_non_finite_propagates`] for the
//! zero-speed domain of the singular rate-and-state laws.

pub mod laws;

use akantu_core::math::{DEFAULT_TOLERANCE, are_float_equal};
use akantu_core::{Array, Error, Idx, Real, Result};
use akantu_mesh::Mesh;
use akantu_model::model::{ReduceOperation, make_consistent_for_periodicity};
use akantu_parser::param::{
    ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry, Parsable,
};
use akantu_parser::{ParserSection, ParserType};

pub use laws::{FrictionLaw, Regularisation, RsArgs, RsCoefficient, RsState};

use crate::TasnUpstreamCompat;
use crate::contact::{NtnBaseContact, norm};
use crate::state::ModelState;
use crate::sync_array::{SyncDependent, SynchronizedArray};

use laws::{cpp_max, flash_heating_slip};

/// Where a registered parameter lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loc {
    /// `frictional_contact_pressure` (regularisation).
    Fcp,
    /// `t_star` (Rubin–Ampuero, simplified Prakash–Clifton).
    TStar,
    /// `spc_internal` (simplified Prakash–Clifton).
    Spc,
    /// `friction_state` (base array, registered by the rate-and-state laws).
    State,
    /// Law parameter `i` (see [`law_param_names`]).
    Law(usize),
}

#[derive(Clone, Copy, Debug)]
struct ParamEntry {
    name: &'static str,
    access: ParameterAccess,
    description: &'static str,
    loc: Loc,
}

/// Law parameter names, in the order of `NtnFriction::law_params`.
fn law_param_names(law: FrictionLaw) -> &'static [&'static str] {
    match law {
        FrictionLaw::Coulomb => &["mu"],
        FrictionLaw::LinearSlipWeakening | FrictionLaw::LinearSlipWeakeningNoHealing => {
            &["mu", "mu_s", "mu_k", "d_c"]
        }
        FrictionLaw::LinearCohesive => &["G_c", "tau_c", "tau_r"],
        FrictionLaw::RateAndState { .. } => {
            &["mu", "mu_0", "a", "b", "v_star", "phi_star", "d_c", "v0"]
        }
        FrictionLaw::RateAndStateFlashHeating => &[
            "mu", "mu_0", "a", "b", "v_star", "phi_star", "d_c", "v0", "mu_w", "v_w",
        ],
    }
}

// indices into law_params
const MU: usize = 0;
const MU_S: usize = 1;
const MU_K: usize = 2;
const LSW_DC: usize = 3;
const G_C: usize = 0;
const TAU_C: usize = 1;
const TAU_R: usize = 2;
const RS_MU0: usize = 1;
const RS_A: usize = 2;
const RS_B: usize = 3;
const RS_VSTAR: usize = 4;
const RS_PHISTAR: usize = 5;
const RS_DC: usize = 6;
const RS_V0: usize = 7;
const FH_MUW: usize = 8;
const FH_VW: usize = 9;

/// The registered parameters, in upstream registration order.
fn registry(law: FrictionLaw, reg: Regularisation) -> Vec<ParamEntry> {
    let p = ParameterAccess::PARSMOD;
    let mut v = Vec::new();
    v.push(ParamEntry {
        name: "frictional_contact_pressure",
        access: ParameterAccess::INTERNAL,
        description: "contact pressure used for friction law",
        loc: Loc::Fcp,
    });
    match reg {
        Regularisation::NoRegularisation => {}
        Regularisation::RubinAmpuero => v.push(ParamEntry {
            name: "t_star",
            access: p,
            description: "time scale of regularization",
            loc: Loc::TStar,
        }),
        Regularisation::SimplifiedPrakashClifton => {
            v.push(ParamEntry {
                name: "t_star",
                access: p,
                description: "time scale of regularisation",
                loc: Loc::TStar,
            });
            v.push(ParamEntry {
                name: "spc_internal",
                access: ParameterAccess::INTERNAL,
                description: "",
                loc: Loc::Spc,
            });
        }
    }
    let law_entry = |i: usize, description: &'static str| ParamEntry {
        name: law_param_names(law)[i],
        access: p,
        description,
        loc: Loc::Law(i),
    };
    match law {
        FrictionLaw::Coulomb => v.push(law_entry(MU, "friction coefficient")),
        FrictionLaw::LinearSlipWeakening | FrictionLaw::LinearSlipWeakeningNoHealing => {
            let mut mu = law_entry(MU, "friction coefficient");
            mu.access = ParameterAccess::READABLE;
            v.push(mu);
            v.push(law_entry(MU_S, "static friction coefficient"));
            v.push(law_entry(MU_K, "kinetic friction coefficient"));
            v.push(law_entry(LSW_DC, "slip weakening length"));
        }
        FrictionLaw::LinearCohesive => {
            v.push(law_entry(G_C, "fracture energy"));
            v.push(law_entry(TAU_C, "peak shear strength"));
            v.push(law_entry(TAU_R, "residual shear strength"));
        }
        FrictionLaw::RateAndState { .. } | FrictionLaw::RateAndStateFlashHeating => {
            let mut mu = law_entry(MU, "friction coefficient");
            mu.access = ParameterAccess::READABLE;
            v.push(mu);
            v.push(law_entry(RS_MU0, "first coefficient of friction"));
            v.push(law_entry(RS_A, "coefficient of second term"));
            v.push(law_entry(RS_B, "coefficient of third term"));
            v.push(law_entry(RS_VSTAR, "velocity coefficient"));
            v.push(law_entry(RS_PHISTAR, "state coefficient"));
            v.push(ParamEntry {
                name: "friction_state",
                access: p,
                description: "state of friction",
                loc: Loc::State,
            });
            v.push(law_entry(RS_DC, "characteristic length"));
            v.push(law_entry(RS_V0, "R&S parameter"));
            if law == FrictionLaw::RateAndStateFlashHeating {
                v.push(law_entry(
                    FH_MUW,
                    "coefficient of friction of flash heating",
                ));
                v.push(law_entry(FH_VW, "velocity of flash heating"));
            }
        }
    }
    v
}

/// Upstream `NTNBaseFriction` + `NTNFriction<Law, Reg>` /
/// `NTRFFriction<Law, Reg>`.
#[derive(Debug, Clone)]
pub struct NtnFriction {
    id: String,
    law: FrictionLaw,
    regularisation: Regularisation,
    /// `NTNFriction` (node-to-node assembly) or `NTRFFriction`.
    node_to_node: bool,
    contact_id: String,
    spatial_dimension: usize,
    is_sticking: SynchronizedArray<bool>,
    frictional_strength: SynchronizedArray<Real>,
    friction_traction: SynchronizedArray<Real>,
    friction_traction_old: SynchronizedArray<Real>,
    slip: SynchronizedArray<Real>,
    cumulative_slip: SynchronizedArray<Real>,
    slip_velocity: SynchronizedArray<Real>,
    slip_velocity_norm: SynchronizedArray<Real>,
    friction_state: SynchronizedArray<Real>,
    frictional_contact_pressure: SynchronizedArray<Real>,
    t_star: Option<SynchronizedArray<Real>>,
    spc_internal: Option<SynchronizedArray<Real>>,
    law_params: Vec<SynchronizedArray<Real>>,
    global_friction_traction: Array<Real>,
    global_friction_traction_old: Array<Real>,
    params: Vec<ParamEntry>,
    synced: usize,
    compat: TasnUpstreamCompat,
}

fn ux(n: Idx) -> Result<usize> {
    usize::try_from(n).map_err(|_| Error::InvalidIndex {
        what: "node",
        value: i64::from(n),
    })
}

impl NtnFriction {
    /// Friction `law` with `regularisation` on `contact` (upstream
    /// `NTNFriction<Law, Reg>(contact, id)` for a node-to-node contact,
    /// `NTRFFriction<Law, Reg>` otherwise; the factory,
    /// [`crate::factory::initialize_ntn_friction`], uses the id `friction`).
    /// The arrays have one default-valued element per synchronised contact
    /// node; `nb_nodes` is the number of mesh nodes (global arrays).
    ///
    /// # Errors
    /// Array creation failure.
    pub fn new(
        contact: &NtnBaseContact,
        nb_nodes: usize,
        law: FrictionLaw,
        regularisation: Regularisation,
        id: &str,
    ) -> Result<Self> {
        let dim = contact.spatial_dimension();
        let sa = |nb: usize, name: &str, default: Real| -> Result<SynchronizedArray<Real>> {
            SynchronizedArray::new(nb, format!("{id}:{name}"), default, name)
        };
        let law_params = law_param_names(law)
            .iter()
            .map(|n| sa(1, n, 0.0))
            .collect::<Result<Vec<_>>>()?;
        let mut f = Self {
            id: id.to_owned(),
            law,
            regularisation,
            node_to_node: contact.is_ntn_contact(),
            contact_id: contact.id().to_owned(),
            spatial_dimension: dim,
            is_sticking: SynchronizedArray::new(
                1,
                format!("{id}:is_sticking"),
                true,
                "is_sticking",
            )?,
            frictional_strength: sa(1, "frictional_strength", 0.0)?,
            friction_traction: sa(dim, "friction_traction", 0.0)?,
            friction_traction_old: sa(dim, "friction_traction_old", 0.0)?,
            slip: sa(1, "slip", 0.0)?,
            cumulative_slip: sa(1, "cumulative_slip", 0.0)?,
            slip_velocity: sa(dim, "slip_velocity", 0.0)?,
            slip_velocity_norm: sa(1, "slip_velocity_norm", 0.0)?,
            friction_state: sa(1, "friction_state", 0.0)?,
            frictional_contact_pressure: sa(1, "frictional_contact_pressure", 0.0)?,
            t_star: if regularisation.has_t_star() {
                Some(sa(1, "t_star", 0.0)?)
            } else {
                None
            },
            spc_internal: if regularisation == Regularisation::SimplifiedPrakashClifton {
                Some(sa(1, "spc_internal", 0.0)?)
            } else {
                None
            },
            law_params,
            global_friction_traction: Array::zeros(nb_nodes, dim),
            global_friction_traction_old: Array::zeros(nb_nodes, dim),
            params: registry(law, regularisation),
            synced: 0,
            compat: contact.compat(),
        };
        f.sync_with(contact)?;
        Ok(f)
    }

    fn arrays_mut(&mut self) -> Vec<&mut dyn SyncDependent> {
        let mut v: Vec<&mut dyn SyncDependent> = vec![
            &mut self.is_sticking,
            &mut self.frictional_strength,
            &mut self.friction_traction,
            &mut self.friction_traction_old,
            &mut self.slip,
            &mut self.cumulative_slip,
            &mut self.slip_velocity,
            &mut self.slip_velocity_norm,
            &mut self.friction_state,
            &mut self.frictional_contact_pressure,
        ];
        if let Some(t) = self.t_star.as_mut() {
            v.push(t);
        }
        if let Some(s) = self.spc_internal.as_mut() {
            v.push(s);
        }
        for p in &mut self.law_params {
            v.push(p);
        }
        v
    }

    /// Replay the contact's node-list changes not yet applied to the friction
    /// arrays (upstream: automatic through `registerDependingArray`), and
    /// check that the friction belongs to `contact` and has one element per
    /// contact node.
    ///
    /// # Errors
    /// Another contact, or the contact has unsynchronised nodes.
    pub fn sync_with(&mut self, contact: &NtnBaseContact) -> Result<()> {
        if contact.id() != self.contact_id {
            return Err(Error::invalid(format!(
                "friction {} belongs to contact {}, not {}",
                self.id,
                self.contact_id,
                contact.id()
            )));
        }
        let journal = contact.journal();
        let since = self.synced;
        if since < journal.len() {
            let mut arrays = self.arrays_mut();
            let pos = journal.replay_since(since, &mut arrays)?;
            self.synced = pos;
        }
        if self.is_sticking.len() != contact.nb_contact_nodes() {
            return Err(Error::invalid(format!(
                "friction {}: {} friction nodes for {} contact nodes (call the contact's sync_arrays after adding nodes)",
                self.id,
                self.is_sticking.len(),
                contact.nb_contact_nodes()
            )));
        }
        Ok(())
    }

    // ---- accessors ------------------------------------------------------

    /// Identifier (upstream `id`).
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The law.
    #[must_use]
    pub fn law(&self) -> FrictionLaw {
        self.law
    }

    /// The regularisation.
    #[must_use]
    pub fn regularisation(&self) -> Regularisation {
        self.regularisation
    }

    /// `true` for `NTNFriction` (node-to-node assembly), `false` for
    /// `NTRFFriction`.
    #[must_use]
    pub fn is_node_to_node(&self) -> bool {
        self.node_to_node
    }

    /// Whether every contact node sticks.
    #[must_use]
    pub fn is_sticking(&self) -> &SynchronizedArray<bool> {
        &self.is_sticking
    }

    /// Mutable sticking flags (upstream `getIsSticking()` non-const).
    pub fn is_sticking_mut(&mut self) -> &mut SynchronizedArray<bool> {
        &mut self.is_sticking
    }

    /// Frictional strength of every contact node.
    #[must_use]
    pub fn frictional_strength(&self) -> &SynchronizedArray<Real> {
        &self.frictional_strength
    }

    /// Friction traction (vector) of every contact node.
    #[must_use]
    pub fn friction_traction(&self) -> &SynchronizedArray<Real> {
        &self.friction_traction
    }

    /// Upstream `friction_traction_old` (never written by the package).
    #[must_use]
    pub fn friction_traction_old(&self) -> &SynchronizedArray<Real> {
        &self.friction_traction_old
    }

    /// Slip since the last sticking.
    #[must_use]
    pub fn slip(&self) -> &SynchronizedArray<Real> {
        &self.slip
    }

    /// Cumulative slip.
    #[must_use]
    pub fn cumulative_slip(&self) -> &SynchronizedArray<Real> {
        &self.cumulative_slip
    }

    /// Slip velocity (vector).
    #[must_use]
    pub fn slip_velocity(&self) -> &SynchronizedArray<Real> {
        &self.slip_velocity
    }

    /// Mutable slip velocity.
    pub fn slip_velocity_mut(&mut self) -> &mut SynchronizedArray<Real> {
        &mut self.slip_velocity
    }

    /// Slip velocity norm.
    #[must_use]
    pub fn slip_velocity_norm(&self) -> &SynchronizedArray<Real> {
        &self.slip_velocity_norm
    }

    /// Mutable slip velocity norm.
    pub fn slip_velocity_norm_mut(&mut self) -> &mut SynchronizedArray<Real> {
        &mut self.slip_velocity_norm
    }

    /// Friction state θ (upstream `getState`).
    #[must_use]
    pub fn friction_state(&self) -> &SynchronizedArray<Real> {
        &self.friction_state
    }

    /// Frictional contact pressure (regularisation output).
    #[must_use]
    pub fn frictional_contact_pressure(&self) -> &SynchronizedArray<Real> {
        &self.frictional_contact_pressure
    }

    /// `t_star` (Rubin–Ampuero, simplified Prakash–Clifton).
    #[must_use]
    pub fn t_star(&self) -> Option<&SynchronizedArray<Real>> {
        self.t_star.as_ref()
    }

    /// `spc_internal` (simplified Prakash–Clifton).
    #[must_use]
    pub fn spc_internal(&self) -> Option<&SynchronizedArray<Real>> {
        self.spc_internal.as_ref()
    }

    /// A law parameter by name (`mu`, `mu_s`, `G_c`, `mu_0`, …).
    #[must_use]
    pub fn law_param(&self, name: &str) -> Option<&SynchronizedArray<Real>> {
        law_param_names(self.law)
            .iter()
            .position(|&n| n == name)
            .map(|i| &self.law_params[i])
    }

    /// Global (per mesh node) friction force.
    #[must_use]
    pub fn global_friction_traction(&self) -> &Array<Real> {
        &self.global_friction_traction
    }

    /// Mutable global friction force.
    pub fn global_friction_traction_mut(&mut self) -> &mut Array<Real> {
        &mut self.global_friction_traction
    }

    /// Saved global friction force.
    #[must_use]
    pub fn global_friction_traction_old(&self) -> &Array<Real> {
        &self.global_friction_traction_old
    }

    /// Upstream `updateFrictionState(node, value)`: `node` is the index of
    /// the contact node.
    ///
    /// # Errors
    /// Index out of range.
    pub fn update_friction_state(&mut self, index: usize, value: Real) -> Result<()> {
        let len = self.friction_state.len();
        if index >= len {
            return Err(Error::OutOfRange {
                what: "contact node",
                index,
                len,
            });
        }
        self.friction_state.set_at(index, value);
        Ok(())
    }

    /// Upstream `savePreviousState()`.
    pub fn save_previous_state(&mut self) {
        self.global_friction_traction_old = self.global_friction_traction.clone();
    }

    /// Upstream `restorePreviousState()`.
    pub fn restore_previous_state(&mut self) {
        self.global_friction_traction = self.global_friction_traction_old.clone();
    }

    fn time_step_checked(state: &ModelState<'_>) -> Result<Real> {
        Ok(state.time_step)
    }

    // ---- algorithms -----------------------------------------------------

    /// Upstream `updateSlip()`.
    ///
    /// # Errors
    /// Missing increment, contact mismatch.
    pub fn update_slip(&mut self, contact: &NtnBaseContact, state: &ModelState<'_>) -> Result<()> {
        self.sync_with(contact)?;
        let dim = self.spatial_dimension;
        let delta_t = Self::time_step_checked(state)?;
        state.check_nodal("updateSlip: increment", &state.increment)?;
        let rel_tan_incr =
            contact.compute_relative_tangential_field(&state.increment, state.nb_nodes())?;
        for n in 0..self.is_sticking.len() {
            let rti = &rel_tan_incr[n * dim..(n + 1) * dim];
            if self.is_sticking.at(n) {
                self.slip.set_at(n, 0.0);
                self.slip_velocity.tuple_mut(n).fill(0.0);
                self.slip_velocity_norm.set_at(n, 0.0);
            } else {
                let s = self.slip.at(n) + norm(rti);
                self.slip.set_at(n, s);
                let c = self.cumulative_slip.at(n) + norm(rti);
                self.cumulative_slip.set_at(n, c);
                let sv = self.slip_velocity.tuple_mut(n);
                for d in 0..dim {
                    sv[d] = rti[d] / delta_t;
                }
                let nrm = norm(sv);
                self.slip_velocity_norm.set_at(n, nrm);
            }
        }
        Ok(())
    }

    /// Upstream `computeStickTraction()`: the tangential traction that would
    /// stop the relative tangential motion of the nodes in contact.
    ///
    /// # Errors
    /// Missing fields, contact mismatch.
    pub fn compute_stick_traction(
        &mut self,
        contact: &NtnBaseContact,
        state: &ModelState<'_>,
    ) -> Result<()> {
        self.sync_with(contact)?;
        let dim = self.spatial_dimension;
        let delta_t = Self::time_step_checked(state)?;
        let nb_nodes = state.nb_nodes();
        state.check_nodal("computeStickTraction: velocity", &state.velocity)?;
        state.check_nodal("computeStickTraction: acceleration", &state.acceleration)?;
        let acceleration = NtnBaseContact::compute_acceleration(state)?;
        let r_velo = contact.compute_relative_tangential_field(&state.velocity, nb_nodes)?;
        let r_acce = contact.compute_relative_tangential_field(&acceleration, nb_nodes)?;
        let r_old_acce =
            contact.compute_relative_tangential_field(&state.acceleration, nb_nodes)?;
        let half_dt = 1.0 / 2.0 * delta_t;
        let impedance = contact.impedance();
        let in_contact = contact.is_in_contact();
        for n in 0..self.is_sticking.len() {
            let c = if in_contact.at(n) { 1.0 } else { 0.0 };
            let s = c * impedance.at(n);
            let t = self.friction_traction.tuple_mut(n);
            for (d, td) in t.iter_mut().enumerate() {
                let i = n * dim + d;
                let gap_dot = r_velo[i] + delta_t * r_acce[i] - half_dt * r_old_acce[i];
                *td = s * gap_dot / 2.0;
            }
        }
        Ok(())
    }

    /// Upstream `computeFrictionTraction()`.
    ///
    /// # Errors
    /// See [`Self::compute_stick_traction`] and the frictional strength
    /// (UD-039 domain error).
    pub fn compute_friction_traction(
        &mut self,
        contact: &NtnBaseContact,
        state: &ModelState<'_>,
    ) -> Result<()> {
        self.compute_stick_traction(contact, state)?;
        self.compute_frictional_strength(contact, state)?;
        let dim = self.spatial_dimension;
        let in_contact = contact.is_in_contact();
        self.is_sticking.zero();
        for n in 0..self.is_sticking.len() {
            if !in_contact.at(n) {
                continue;
            }
            let t = self.friction_traction.tuple_mut(n);
            let abs_fric = norm(&t[..dim]);
            if abs_fric != 0.0 {
                let alpha = self.frictional_strength.at(n) / abs_fric;
                if alpha < 1.0 {
                    for v in t.iter_mut() {
                        *v *= alpha;
                    }
                } else {
                    self.is_sticking.set_at(n, true);
                }
            } else {
                self.is_sticking.set_at(n, true);
            }
        }
        Ok(())
    }

    /// Upstream `computeFrictionalStrength()` of the law/regularisation
    /// stack.
    ///
    /// # Errors
    /// Contact mismatch, or (UD-039, corrected mode) a rate-and-state
    /// coefficient that is not finite at a node in contact.
    pub fn compute_frictional_strength(
        &mut self,
        contact: &NtnBaseContact,
        state: &ModelState<'_>,
    ) -> Result<()> {
        self.sync_with(contact)?;
        match self.law {
            FrictionLaw::Coulomb => self.coulomb_strength(contact, state),
            FrictionLaw::LinearSlipWeakening => {
                self.linear_slip_weakening_coefficient(false);
                self.coulomb_strength(contact, state)
            }
            FrictionLaw::LinearSlipWeakeningNoHealing => {
                self.linear_slip_weakening_coefficient(true);
                self.coulomb_strength(contact, state)
            }
            FrictionLaw::LinearCohesive => self.linear_cohesive_strength(contact, state),
            FrictionLaw::RateAndState {
                coefficient,
                state: st,
            } => {
                self.rate_and_state_coefficient(coefficient, st, state.time_step);
                self.coulomb_strength(contact, state)
            }
            FrictionLaw::RateAndStateFlashHeating => {
                self.flash_heating_coefficient(state.time_step);
                self.coulomb_strength(contact, state)
            }
        }
    }

    fn linear_slip_weakening_coefficient(&mut self, no_healing: bool) {
        for n in 0..self.is_sticking.len() {
            let mu_s = self.law_params[MU_S].at(n);
            let mu_k = self.law_params[MU_K].at(n);
            let d_c = self.law_params[LSW_DC].at(n);
            let mu = if !no_healing && self.is_sticking.at(n) {
                mu_s
            } else {
                let slip = if no_healing {
                    self.cumulative_slip.at(n)
                } else {
                    self.slip.at(n)
                };
                if slip >= d_c {
                    mu_k
                } else {
                    mu_k + (1.0 - (slip / d_c)) * (mu_s - mu_k)
                }
            };
            self.law_params[MU].set_at(n, mu);
        }
    }

    fn rs_args(&self, n: usize, time_step: Real) -> RsArgs {
        RsArgs {
            mu_0: self.law_params[RS_MU0].at(n),
            a: self.law_params[RS_A].at(n),
            b: self.law_params[RS_B].at(n),
            v_star: self.law_params[RS_VSTAR].at(n),
            phi_star: self.law_params[RS_PHISTAR].at(n),
            d_c: self.law_params[RS_DC].at(n),
            v0: self.law_params[RS_V0].at(n),
            slip_velocity: self.slip_velocity_norm.at(n),
            state: self.friction_state.at(n),
            time_step,
        }
    }

    fn rate_and_state_coefficient(&mut self, coefficient: RsCoefficient, st: RsState, dt: Real) {
        for n in 0..self.is_sticking.len() {
            let mut args = self.rs_args(n, dt);
            if !self.is_sticking.at(n) {
                let s = args.state + st.increment(&args);
                self.friction_state.set_at(n, s);
                args.state = s;
            }
            let mu = coefficient.evaluate(&args);
            self.law_params[MU].set_at(n, mu);
        }
    }

    fn flash_heating_coefficient(&mut self, dt: Real) {
        for n in 0..self.is_sticking.len() {
            let args = self.rs_args(n, dt);
            if self.is_sticking.at(n) {
                self.law_params[MU].set_at(n, args.mu_0);
            } else {
                let (s, mu) = flash_heating_slip(
                    &args,
                    self.law_params[FH_MUW].at(n),
                    self.law_params[FH_VW].at(n),
                );
                self.friction_state.set_at(n, s);
                self.law_params[MU].set_at(n, mu);
            }
        }
    }

    /// Upstream `internalGetContactPressure()` of the regularisation.
    fn regularised_contact_pressure(&mut self, contact: &NtnBaseContact, delta_t: Real) {
        let dim = self.spatial_dimension;
        let in_contact = contact.is_in_contact();
        let pressure = contact.contact_pressure();
        let nb = self.is_sticking.len();
        match self.regularisation {
            Regularisation::NoRegularisation | Regularisation::SimplifiedPrakashClifton => {
                self.compute_frictional_contact_pressure(contact);
            }
            Regularisation::RubinAmpuero => {
                let t_star = self
                    .t_star
                    .as_ref()
                    .map(SynchronizedArray::as_slice)
                    .unwrap_or(&[]);
                for (n, &ts) in t_star.iter().enumerate().take(nb) {
                    if !in_contact.at(n) {
                        self.frictional_contact_pressure.set_at(n, 0.0);
                    } else if are_float_equal(ts, 0.0, DEFAULT_TOLERANCE) {
                        self.frictional_contact_pressure
                            .set_at(n, norm(&pressure.tuple(n)[..dim]));
                    } else {
                        let alpha = delta_t / ts;
                        let mut p = self.frictional_contact_pressure.at(n);
                        p += alpha * norm(&pressure.tuple(n)[..dim]);
                        p /= 1.0 + alpha;
                        self.frictional_contact_pressure.set_at(n, p);
                    }
                }
            }
        }
    }

    /// Upstream `NTNFricRegNoRegularisation::computeFrictionalContactPressure`.
    fn compute_frictional_contact_pressure(&mut self, contact: &NtnBaseContact) {
        let dim = self.spatial_dimension;
        let in_contact = contact.is_in_contact();
        let pressure = contact.contact_pressure();
        for n in 0..self.is_sticking.len() {
            if in_contact.at(n) {
                self.frictional_contact_pressure
                    .set_at(n, norm(&pressure.tuple(n)[..dim]));
            } else {
                self.frictional_contact_pressure.set_at(n, 0.0);
            }
        }
    }

    /// The array the law writes its strength to (upstream
    /// `internalGetFrictionalStrength`: `spc_internal` for the simplified
    /// Prakash–Clifton regularisation).
    fn strength_target(&mut self) -> &mut SynchronizedArray<Real> {
        match self.spc_internal.as_mut() {
            Some(s) => s,
            None => &mut self.frictional_strength,
        }
    }

    /// The UD-039 domain error of a non-finite rate-and-state coefficient
    /// at contact node `n`, naming its cause: a zero slip
    /// velocity in a law singular at rest (`ln 0`), or a state that left the
    /// domain of the logarithm (θ ≤ 0 for the Dieterich / revised laws,
    /// 1 + θ/θ* ≤ 0 for the regularised ones) because upstream's
    /// explicit-Euler ageing update `θ ← θ(1 − VΔt/d_c) + Δt` is unstable for
    /// `VΔt/d_c > 1`.
    fn rate_and_state_domain_error(&self, n: usize, mu: Real, delta_t: Real) -> Error {
        let theta = self.friction_state.at(n);
        let v = self.slip_velocity_norm.at(n);
        let ratio = v * delta_t / self.law_params[RS_DC].at(n);
        let singular_at_rest = match self.law {
            FrictionLaw::RateAndState { coefficient, .. } => coefficient.singular_at_rest(),
            _ => false,
        };
        let cause = if theta.is_nan() || theta < 0.0 {
            format!(
                "negative state θ = {theta:e} (upstream's explicit-Euler ageing update \
                 θ ← θ(1 − VΔt/d_c) + Δt is unstable when VΔt/d_c > 1; here VΔt/d_c = {ratio:e}): \
                 reduce the time step or increase d_c"
            )
        } else if v == 0.0 && singular_at_rest {
            "zero slip velocity: ln 0 of a law singular at rest (dieterich, n_shaped): use a \
             law regular at rest"
                .to_owned()
        } else {
            format!("θ = {theta:e}, VΔt/d_c = {ratio:e}")
        };
        Error::Numerical(format!(
            "friction {}: the rate-and-state coefficient of contact node {n} (in contact) is {mu} \
             with slip velocity V = {v:e} (outside the domain of the law, UD-039); cause: {cause}. \
             The compat switch rate_and_state_non_finite_propagates reproduces upstream",
            self.id
        ))
    }

    fn coulomb_strength(&mut self, contact: &NtnBaseContact, state: &ModelState<'_>) -> Result<()> {
        let delta_t = state.time_step;
        self.regularised_contact_pressure(contact, delta_t);
        let in_contact = contact.is_in_contact();
        let nb = self.is_sticking.len();
        let check =
            self.law.is_rate_and_state() && !self.compat.rate_and_state_non_finite_propagates;
        let mut bad: Option<(usize, Real)> = None;
        let mu: Vec<Real> = self.law_params[MU].as_slice().to_vec();
        let pressure: Vec<Real> = self.frictional_contact_pressure.as_slice().to_vec();
        let target = self.strength_target();
        for n in 0..nb {
            if !in_contact.at(n) {
                target.set_at(n, 0.0);
            } else {
                if check && bad.is_none() && !mu[n].is_finite() {
                    bad = Some((n, mu[n]));
                }
                target.set_at(n, mu[n] * pressure[n]);
            }
        }
        if let Some((n, m)) = bad {
            return Err(self.rate_and_state_domain_error(n, m, delta_t));
        }
        self.regularisation_strength(state)
    }

    fn linear_cohesive_strength(
        &mut self,
        contact: &NtnBaseContact,
        state: &ModelState<'_>,
    ) -> Result<()> {
        let in_contact = contact.is_in_contact();
        let nb = self.is_sticking.len();
        let g_c: Vec<Real> = self.law_params[G_C].as_slice().to_vec();
        let tau_c: Vec<Real> = self.law_params[TAU_C].as_slice().to_vec();
        let tau_r: Vec<Real> = self.law_params[TAU_R].as_slice().to_vec();
        let slip: Vec<Real> = self.cumulative_slip.as_slice().to_vec();
        let target = self.strength_target();
        for n in 0..nb {
            if !in_contact.at(n) {
                target.set_at(n, 0.0);
            } else if g_c[n] == 0.0 {
                target.set_at(n, tau_r[n]);
            } else {
                let slope = (tau_c[n] - tau_r[n]) * (tau_c[n] - tau_r[n]) / (2.0 * g_c[n]);
                target.set_at(n, cpp_max(tau_c[n] - slope * slip[n], tau_r[n]));
            }
        }
        self.regularisation_strength(state)
    }

    /// Upstream `Regularisation::computeFrictionalStrength()` (only the
    /// simplified Prakash–Clifton regularisation does something).
    ///
    /// **UD-397 (B):** with `t* = 0` (its default) upstream computes
    /// `α = Δt/0 = ∞` and a NaN strength; the port uses the `t* → 0` limit
    /// (the unregularised strength) when `t*` is zero to the package
    /// tolerance, as the Rubin–Ampuero regularisation does; the compat switch
    /// [`TasnUpstreamCompat::spc_zero_t_star_nan`] keeps upstream's formula.
    fn regularisation_strength(&mut self, state: &ModelState<'_>) -> Result<()> {
        if self.regularisation != Regularisation::SimplifiedPrakashClifton {
            return Ok(());
        }
        let delta_t = state.time_step;
        let (Some(t_star), Some(spc)) = (self.t_star.as_ref(), self.spc_internal.as_ref()) else {
            return Err(Error::invalid("simplified Prakash-Clifton arrays missing"));
        };
        let compat = self.compat.spc_zero_t_star_nan;
        for n in 0..self.is_sticking.len() {
            let ts = t_star.at(n);
            if !compat && are_float_equal(ts, 0.0, DEFAULT_TOLERANCE) {
                self.frictional_strength.set_at(n, spc.at(n));
                continue;
            }
            let alpha = delta_t / ts;
            let mut s = self.frictional_strength.at(n);
            s += alpha * spc.at(n);
            s /= 1.0 + alpha;
            self.frictional_strength.set_at(n, s);
        }
        Ok(())
    }

    /// Upstream `setToSteadyState()`: no regularisation, nothing;
    /// Rubin–Ampuero, the unregularised frictional pressure; simplified
    /// Prakash–Clifton, the full strength computation then the
    /// unregularised strength.
    ///
    /// # Errors
    /// See [`Self::compute_frictional_strength`].
    pub fn set_to_steady_state(
        &mut self,
        contact: &NtnBaseContact,
        state: &ModelState<'_>,
    ) -> Result<()> {
        self.sync_with(contact)?;
        match self.regularisation {
            Regularisation::NoRegularisation => Ok(()),
            Regularisation::RubinAmpuero => {
                self.compute_frictional_contact_pressure(contact);
                Ok(())
            }
            Regularisation::SimplifiedPrakashClifton => {
                self.compute_frictional_strength(contact, state)?;
                if let Some(spc) = self.spc_internal.as_ref() {
                    let v = spc.as_slice().to_vec();
                    self.frictional_strength.as_mut_slice().copy_from_slice(&v);
                }
                Ok(())
            }
        }
    }

    /// Upstream `assembleGlobalFrictionTraction()`: node-to-node, master
    /// `+= A_m τ`, slave `−= A_s τ`; rigid flat, slave `= −A_s τ`; then made
    /// consistent for periodicity.
    ///
    /// # Errors
    /// Contact mismatch or inconsistent sizes.
    pub fn assemble_global_friction_traction(
        &mut self,
        contact: &NtnBaseContact,
        mesh: &Mesh,
    ) -> Result<()> {
        self.sync_with(contact)?;
        let dim = self.spatial_dimension;
        crate::state::check_len(
            "assembleGlobalFrictionTraction: global array",
            self.global_friction_traction.as_slice(),
            mesh.nb_nodes() * dim,
        )?;
        self.global_friction_traction.zero();
        let slaves = contact.slaves();
        let lbs = contact.lumped_boundary_slaves();
        let nb = self.is_sticking.len();
        let gft = self.global_friction_traction.as_mut_slice();
        if self.node_to_node {
            let (Some(masters), Some(lbm)) = (contact.masters(), contact.lumped_boundary_masters())
            else {
                return Err(Error::invalid(
                    "NTNFriction::assembleGlobalFrictionTraction needs a node-to-node contact",
                ));
            };
            for n in 0..nb {
                let ft = self.friction_traction.tuple(n);
                let m = ux(masters.at(n))?;
                let s = ux(slaves.at(n))?;
                for d in 0..dim {
                    gft[m * dim + d] += lbm.at(n) * ft[d];
                }
                for d in 0..dim {
                    gft[s * dim + d] -= lbs.at(n) * ft[d];
                }
            }
        } else {
            for n in 0..nb {
                let ft = self.friction_traction.tuple(n);
                let s = ux(slaves.at(n))?;
                for d in 0..dim {
                    gft[s * dim + d] = -lbs.at(n) * ft[d];
                }
            }
        }
        make_consistent_for_periodicity(
            mesh,
            &mut self.global_friction_traction,
            ReduceOperation::Id,
            |a, _| a,
        )
    }

    /// Upstream `getNbStickingNodes()`: contact nodes in contact and sticking
    /// that are local or master nodes and not periodic slaves, summed over
    /// the ranks of a distributed mesh (upstream `allReduce(_sum)`;
    /// collective there).
    ///
    /// # Errors
    /// Contact mismatch or invalid node (agreed on every rank of a
    /// distributed mesh).
    pub fn nb_sticking_nodes(&mut self, contact: &NtnBaseContact, mesh: &Mesh) -> Result<usize> {
        let local = self.sync_with(contact).and_then(|()| {
            let mut nb = 0;
            let in_contact = contact.is_in_contact();
            for (n, &s) in contact.slaves().as_slice().iter().enumerate() {
                if mesh.is_local_or_master_node(s)?
                    && !mesh.is_periodic_slave(s)?
                    && in_contact.at(n)
                    && self.is_sticking.at(n)
                {
                    nb += 1;
                }
            }
            Ok(nb)
        });
        crate::parallel::sum_count(mesh, local, "nb_sticking_nodes")
    }

    /// Upstream `setParam(name, node, value)`: the value of one node of a
    /// readable and writable per-node parameter; a node that is not a contact
    /// node is ignored with a warning, as upstream.
    ///
    /// # Errors
    /// Unknown parameter, or a parameter that is not accessible.
    pub fn set_param_at_node(
        &mut self,
        contact: &NtnBaseContact,
        name: &str,
        node: Idx,
        value: Real,
    ) -> Result<()> {
        self.sync_with(contact)?;
        let entry = self.entry(name)?;
        if !entry.access.contains(ParameterAccess::READABLE)
            || !entry.access.contains(ParameterAccess::WRITABLE)
        {
            return Err(Error::invalid(format!(
                "The parameter named {name} is not accessible"
            )));
        }
        match contact.node_index(node) {
            None => {
                log::warn!(
                    "Node {node} is not a contact node. Therefore, cannot set interface parameter!!"
                );
                Ok(())
            }
            Some(i) => {
                let loc = entry.loc;
                let a = self.loc_array_mut(loc)?;
                a.set_at(i, value);
                Ok(())
            }
        }
    }

    /// Upstream `addNoiseToStateVariable(noise)` (rate-and-state laws):
    /// `θ ← θ (1 + perturbation)` with the perturbation of the first key not
    /// below the node's x coordinate (current position).
    ///
    /// **UD-396 (A):** the base-class version calls itself forever (stack
    /// overflow) for the other laws, and the rate-and-state version
    /// dereferences `end()` when a node lies beyond the last key; both are
    /// errors here.
    ///
    /// # Errors
    /// Not a rate-and-state law, no key at or above a node's x coordinate,
    /// NaN keys.
    pub fn add_noise_to_state_variable(
        &mut self,
        contact: &NtnBaseContact,
        state: &ModelState<'_>,
        noise: &[(Real, Real)],
    ) -> Result<()> {
        self.sync_with(contact)?;
        if !self.law.is_rate_and_state() {
            return Err(Error::NotImplemented(
                "NTNBaseFriction::addNoiseToStateVariable for laws without a state variable \
                 (upstream recurses infinitely, UD-396)",
            ));
        }
        if noise.iter().any(|(k, _)| k.is_nan()) {
            return Err(Error::invalid("addNoiseToStateVariable: NaN key"));
        }
        // std::map semantics: sorted unique keys, the first value of a key
        let mut map: Vec<(Real, Real)> = noise.to_vec();
        map.sort_by(|a, b| a.0.total_cmp(&b.0));
        map.dedup_by(|a, b| a.0 == b.0);
        log::info!("Adding noise to the state field");
        let cur_pos = state.current_position()?;
        let dim = self.spatial_dimension;
        for (n, &slave) in contact.slaves().as_slice().iter().enumerate() {
            let x = cur_pos[ux(slave)? * dim];
            let k = map.partition_point(|(key, _)| *key < x);
            let Some(&(_, perturbation)) = map.get(k) else {
                return Err(Error::invalid(format!(
                    "addNoiseToStateVariable: no noise key at or above x = {x} (slave {slave}; UD-396)"
                )));
            };
            log::info!("Slave :{slave} {x} : perturbation is {perturbation}");
            let f = self.friction_state.at(n);
            self.friction_state.set_at(n, f * (1.0 + perturbation));
        }
        Ok(())
    }

    // ---- parameters -----------------------------------------------------

    fn entry(&self, name: &str) -> Result<ParamEntry> {
        self.params
            .iter()
            .find(|e| e.name == name)
            .copied()
            .ok_or_else(|| Error::not_found("parameter", format!("{name} in {}", self.id)))
    }

    fn loc_array(&self, loc: Loc) -> Result<&SynchronizedArray<Real>> {
        match loc {
            Loc::Fcp => Ok(&self.frictional_contact_pressure),
            Loc::TStar => self
                .t_star
                .as_ref()
                .ok_or_else(|| Error::not_found("parameter", "t_star")),
            Loc::Spc => self
                .spc_internal
                .as_ref()
                .ok_or_else(|| Error::not_found("parameter", "spc_internal")),
            Loc::State => Ok(&self.friction_state),
            Loc::Law(i) => self
                .law_params
                .get(i)
                .ok_or_else(|| Error::not_found("parameter", "law parameter")),
        }
    }

    fn loc_array_mut(&mut self, loc: Loc) -> Result<&mut SynchronizedArray<Real>> {
        match loc {
            Loc::Fcp => Ok(&mut self.frictional_contact_pressure),
            Loc::TStar => self
                .t_star
                .as_mut()
                .ok_or_else(|| Error::not_found("parameter", "t_star")),
            Loc::Spc => self
                .spc_internal
                .as_mut()
                .ok_or_else(|| Error::not_found("parameter", "spc_internal")),
            Loc::State => Ok(&mut self.friction_state),
            Loc::Law(i) => self
                .law_params
                .get_mut(i)
                .ok_or_else(|| Error::not_found("parameter", "law parameter")),
        }
    }

    /// Description of a registered parameter (upstream `registerParam`
    /// description).
    ///
    /// # Errors
    /// Unknown parameter.
    pub fn param_description(&self, name: &str) -> Result<&'static str> {
        Ok(self.entry(name)?.description)
    }

    /// The per-node array of a registered parameter (any access).
    ///
    /// # Errors
    /// Unknown parameter.
    pub fn param_array(&self, name: &str) -> Result<&SynchronizedArray<Real>> {
        let e = self.entry(name)?;
        self.loc_array(e.loc)
    }

    /// Upstream `Parsable::parseSection` of a `friction` section.
    ///
    /// # Errors
    /// Wrong section type, unknown or non-parsable parameter (unless
    /// `permissive`), unconvertible value.
    pub fn parse_friction_section(
        &mut self,
        section: &ParserSection,
        permissive: bool,
    ) -> Result<()> {
        self.parse_section(section, permissive)
    }

    // ---- restart ---------------------------------------------------------

    fn restart_arrays(&self, reading: bool) -> Vec<&SynchronizedArray<Real>> {
        // law (most derived first), then regularisation, then base, in the
        // order of the upstream dumpRestart chain
        let mut v: Vec<&SynchronizedArray<Real>> = Vec::new();
        let lp = &self.law_params;
        match self.law {
            FrictionLaw::RateAndStateFlashHeating => {
                v.extend([&lp[FH_MUW], &lp[FH_VW]]);
                v.extend([
                    &lp[RS_MU0],
                    &lp[RS_A],
                    &lp[RS_B],
                    &lp[RS_PHISTAR],
                    &lp[RS_VSTAR],
                    &lp[RS_DC],
                    &lp[RS_V0],
                ]);
                v.push(&self.friction_state);
                v.push(&lp[MU]);
            }
            FrictionLaw::RateAndState { .. } => {
                v.extend([
                    &lp[RS_MU0],
                    &lp[RS_A],
                    &lp[RS_B],
                    &lp[RS_PHISTAR],
                    &lp[RS_VSTAR],
                    &lp[RS_DC],
                    &lp[RS_V0],
                ]);
                v.push(&self.friction_state);
                v.push(&lp[MU]);
            }
            FrictionLaw::LinearSlipWeakening | FrictionLaw::LinearSlipWeakeningNoHealing => {
                v.extend([&lp[MU_S], &lp[MU_K], &lp[LSW_DC], &lp[MU]]);
            }
            FrictionLaw::Coulomb => v.push(&lp[MU]),
            FrictionLaw::LinearCohesive => v.extend([&lp[G_C], &lp[TAU_C], &lp[TAU_R]]),
        }
        if let Some(t) = &self.t_star {
            v.push(t);
        }
        if let Some(s) = &self.spc_internal {
            v.push(s);
        }
        v.push(&self.frictional_contact_pressure);
        // base: is_sticking (bool, handled separately), strength, tractions,
        // slip (not read upstream: UD-395), cumulative slip, slip velocity
        v.push(&self.frictional_strength);
        v.push(&self.friction_traction);
        v.push(&self.friction_traction_old);
        if !reading || !self.compat.friction_restart_skips_slip {
            v.push(&self.slip);
        }
        v.push(&self.cumulative_slip);
        v.push(&self.slip_velocity);
        v
    }

    /// Upstream `dumpRestart(file_name)` of the law/regularisation stack.
    ///
    /// # Errors
    /// Contact mismatch, unsynchronised arrays or I/O failure.
    pub fn dump_restart(&mut self, contact: &NtnBaseContact, file_name: &str) -> Result<()> {
        self.sync_with(contact)?;
        for a in self.restart_arrays(false) {
            a.dump_restart_file(file_name)?;
        }
        self.is_sticking.dump_restart_file(file_name)
    }

    /// Upstream `readRestart(file_name)`.
    ///
    /// **UD-395 (B):** upstream `NTNBaseFriction::readRestart` does not read
    /// the `slip` file that `dumpRestart` writes, so a restarted slip
    /// weakening law restarts from zero slip; the port reads it (compat:
    /// [`TasnUpstreamCompat::friction_restart_skips_slip`]). UD-394 applies
    /// to every array.
    ///
    /// # Errors
    /// Contact mismatch, unreadable or inconsistent files.
    pub fn read_restart(&mut self, contact: &NtnBaseContact, file_name: &str) -> Result<()> {
        self.sync_with(contact)?;
        let ig = self.compat.restart_read_ignores_values;
        let names: Vec<String> = self
            .restart_arrays(true)
            .iter()
            .map(|a| a.id().to_owned())
            .collect();
        for id in names {
            let a = self.array_by_id_mut(&id)?;
            a.read_restart_file(file_name, ig)?;
        }
        self.is_sticking.read_restart_file(file_name, ig)
    }

    fn array_by_id_mut(&mut self, id: &str) -> Result<&mut SynchronizedArray<Real>> {
        let mut all: Vec<&mut SynchronizedArray<Real>> = vec![
            &mut self.frictional_strength,
            &mut self.friction_traction,
            &mut self.friction_traction_old,
            &mut self.slip,
            &mut self.cumulative_slip,
            &mut self.slip_velocity,
            &mut self.slip_velocity_norm,
            &mut self.friction_state,
            &mut self.frictional_contact_pressure,
        ];
        if let Some(t) = self.t_star.as_mut() {
            all.push(t);
        }
        if let Some(s) = self.spc_internal.as_mut() {
            all.push(s);
        }
        for p in &mut self.law_params {
            all.push(p);
        }
        all.into_iter()
            .find(|a| a.id() == id)
            .ok_or_else(|| Error::not_found("friction array", id))
    }

    /// Nodal arrays of the friction by dump-field name (upstream
    /// `addDumpFieldToDumper` chain): `is_sticking` is boolean and returned
    /// by [`Self::sticking_field`]; the global arrays are per mesh node.
    #[must_use]
    pub fn friction_field(&self, field_id: &str) -> Option<(&[Real], usize)> {
        let dim = self.spatial_dimension;
        let law_field = |name: &str| self.law_param(name).map(|a| (a.as_slice(), 1));
        match field_id {
            "frictional_strength" => Some((self.frictional_strength.as_slice(), 1)),
            "friction_traction" => Some((self.friction_traction.as_slice(), dim)),
            "friction_traction_old" => Some((self.friction_traction_old.as_slice(), dim)),
            "global_friction_traction" => Some((self.global_friction_traction.as_slice(), dim)),
            "global_friction_traction_old" => {
                Some((self.global_friction_traction_old.as_slice(), dim))
            }
            "slip" => Some((self.slip.as_slice(), 1)),
            "cumulative_slip" => Some((self.cumulative_slip.as_slice(), 1)),
            "slip_velocity" => Some((self.slip_velocity.as_slice(), dim)),
            "slip_velocity_norm" => Some((self.slip_velocity_norm.as_slice(), 1)),
            "frictional_contact_pressure" => Some((self.frictional_contact_pressure.as_slice(), 1)),
            "t_star" => self.t_star.as_ref().map(|a| (a.as_slice(), 1)),
            "mu" if self.law.is_coulomb_family() => law_field("mu"),
            "mu_s" | "mu_k" | "d_c"
                if matches!(
                    self.law,
                    FrictionLaw::LinearSlipWeakening | FrictionLaw::LinearSlipWeakeningNoHealing
                ) =>
            {
                law_field(field_id)
            }
            "G_c" | "tau_c" | "tau_r" if self.law == FrictionLaw::LinearCohesive => {
                law_field(field_id)
            }
            // upstream: the flash-heating law forwards to the Coulomb law, so
            // only its own `mu_w`, `v_w` (and `mu`) are dumpable
            "mu_w" | "v_w" if self.law == FrictionLaw::RateAndStateFlashHeating => {
                law_field(field_id)
            }
            "mu_0" | "a" | "b" | "phi_star" | "v_star" | "d_c"
                if matches!(self.law, FrictionLaw::RateAndState { .. }) =>
            {
                law_field(field_id)
            }
            "friction_state" if matches!(self.law, FrictionLaw::RateAndState { .. }) => {
                Some((self.friction_state.as_slice(), 1))
            }
            _ => None,
        }
    }

    /// The `is_sticking` dump field.
    #[must_use]
    pub fn sticking_field(&self) -> &[bool] {
        self.is_sticking.as_slice()
    }
}

impl ParameterRegistry for NtnFriction {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        let e = self.entry(name)?;
        if !e.access.contains(ParameterAccess::READABLE) {
            return Err(Error::invalid(format!(
                "The parameter named {name} is not accessible"
            )));
        }
        // upstream ParameterTyped<SynchronizedArray<Real>>::operator Real():
        // the array's default value
        Ok(ParamValue::Real(self.loc_array(e.loc)?.default_value()))
    }

    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        let e = self.entry(name)?;
        if !e.access.contains(ParameterAccess::WRITABLE) {
            return Err(Error::invalid(format!(
                "The parameter named {name} is not writable"
            )));
        }
        let v = value.as_real()?;
        self.loc_array_mut(e.loc)?.set_and_change_default(v);
        Ok(())
    }

    fn param_names(&self) -> Vec<&'static str> {
        self.params.iter().map(|e| e.name).collect()
    }

    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.params
            .iter()
            .find(|e| e.name == name)
            .map(|e| ParamInfo {
                kind: ParamKind::Real,
                access: e.access,
            })
    }

    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        let e = self.entry(name)?;
        if !e.access.contains(ParameterAccess::PARSABLE) {
            return Err(Error::invalid(format!("Parameter {name} is not parsable")));
        }
        let v = value.as_real()?;
        self.loc_array_mut(e.loc)?.set_and_change_default(v);
        Ok(())
    }
}

impl Parsable for NtnFriction {
    fn section_type(&self) -> ParserType {
        ParserType::Friction
    }

    fn parsable_id(&self) -> &str {
        &self.id
    }
}

impl std::fmt::Display for NtnFriction {
    /// Upstream `printself` of the template stack (simplified layout).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let outer = if self.node_to_node {
            "NTNFriction"
        } else {
            "NTRFFriction"
        };
        writeln!(f, "{outer} [")?;
        writeln!(f, " {} [", self.law.class_name())?;
        writeln!(f, "  {} [", self.regularisation.class_name())?;
        writeln!(f, "   NTNBaseFriction [ {} ]", self.id)?;
        for e in &self.params {
            if let Ok(a) = self.loc_array(e.loc) {
                writeln!(
                    f,
                    "   {} [{}] : {}",
                    e.name,
                    e.access.flags_string(),
                    a.default_value()
                )?;
            }
        }
        writeln!(f, "  ]\n ]\n]")
    }
}
