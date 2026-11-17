// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_cohesive/materials/constitutive_laws/*.

//! The cohesive constitutive laws (CMAT-001…006) behind one enum: the
//! common state is [`crate::material::CohesiveCore`], each variant holds
//! the law's parameters and internals.

pub mod foreign;
pub mod linear;
pub mod others;

use akantu_core::{Element, ElementType, ElementTypeMapArray, Error, GhostType, Int, Real, Result};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::InternalField;
use akantu_model::constitutive_law::LawContext;
use akantu_parser::param::ParamDescriptor;
use akantu_solid::material::MaterialBase;

use crate::material::{CohesiveCore, Domain, MaterialCohesive, P_DELTA_C, P_SIGMA_C};
use foreign::ForeignCohesive;
use linear::{
    Bilinear, Linear, P_BETA, P_CONTACT_AFTER_BREAKING, P_DELTA_0, P_GC, P_KAPPA, P_M_S,
    P_MAX_QUAD, P_PENALTY, P_RECOMPUTE, P_VOLUME_S, Penetration,
};
use others::{Exponential, Fatigue, Friction, Uncoupled};

/// Registered names of the cohesive laws.
pub const LAW_NAMES: [&str; 6] = [
    "cohesive_linear",
    "cohesive_bilinear",
    "cohesive_exponential",
    "cohesive_linear_fatigue",
    "cohesive_linear_friction",
    "cohesive_linear_uncoupled",
];

#[derive(Debug)]
/// Inputs of a law's insertion check (upstream reads them from the model
/// and the inserter).
pub struct InsertionInputs<'a> {
    /// The facet mesh.
    pub mesh_facets: &'a Mesh,
    /// Facets where the check is performed.
    pub check_facets: &'a ElementTypeMapArray<bool>,
    /// Facets flagged for insertion (written).
    pub insertion_facets: &'a mut ElementTypeMapArray<bool>,
    /// Stresses of both sides at the facet quadrature points.
    pub facet_stress: &'a ElementTypeMapArray<Real>,
    /// Tangents at the facet quadrature points.
    pub tangents: &'a ElementTypeMapArray<Real>,
    /// The facet FE engine (normals at the quadrature points).
    pub facet_fe: &'a FeEngine,
}

static LINEAR_PARAMS: [ParamDescriptor<MaterialCohesive>; 11] = [
    P_SIGMA_C,
    P_DELTA_C,
    P_BETA,
    P_GC,
    P_PENALTY,
    P_VOLUME_S,
    P_M_S,
    P_KAPPA,
    P_CONTACT_AFTER_BREAKING,
    P_MAX_QUAD,
    P_RECOMPUTE,
];

static BILINEAR_PARAMS: [ParamDescriptor<MaterialCohesive>; 12] = [
    P_SIGMA_C,
    P_DELTA_C,
    P_BETA,
    P_GC,
    P_PENALTY,
    P_VOLUME_S,
    P_M_S,
    P_KAPPA,
    P_CONTACT_AFTER_BREAKING,
    P_MAX_QUAD,
    P_RECOMPUTE,
    P_DELTA_0,
];

macro_rules! law_param {
    ($name:literal, $kind:expr, $access:expr, $desc:literal, $variant:ident, $field:ident, $get:expr, $conv:ident) => {
        ParamDescriptor::new(
            $name,
            $kind,
            $access,
            $desc,
            |s| match &s.law {
                CohesiveLaw::$variant(l) => $get(l.$field),
                _ => $get(Default::default()),
            },
            Some(|s, v| match &mut s.law {
                CohesiveLaw::$variant(l) => {
                    l.$field = v.$conv()?;
                    Ok(())
                }
                _ => Err(Error::invalid(concat!(
                    $name,
                    " does not belong to this cohesive law"
                ))),
            }),
        )
    };
}

use crate::material::PAR_READ;
use akantu_parser::param::{ParamKind, ParamValue, ParameterAccess};

const P_EXP_BETA: ParamDescriptor<MaterialCohesive> = law_param!(
    "beta",
    ParamKind::Real,
    ParameterAccess::PARSABLE,
    "Beta parameter",
    Exponential,
    beta,
    ParamValue::Real,
    as_real
);
const P_EXP_PENALTY: ParamDescriptor<MaterialCohesive> = law_param!(
    "exponential_penalty",
    ParamKind::Bool,
    ParameterAccess::PARSABLE,
    "Is contact penalty following the exponential law?",
    Exponential,
    exp_penalty,
    ParamValue::Bool,
    as_bool
);
const P_EXP_CT: ParamDescriptor<MaterialCohesive> = law_param!(
    "contact_tangent",
    ParamKind::Real,
    ParameterAccess::PARSABLE,
    "Ratio of contact tangent over the initial exponential tangent",
    Exponential,
    contact_tangent,
    ParamValue::Real,
    as_real
);
const P_DELTA_F: ParamDescriptor<MaterialCohesive> = law_param!(
    "delta_f",
    ParamKind::Real,
    PAR_READ,
    "delta_f",
    LinearFatigue,
    delta_f,
    ParamValue::Real,
    as_real
);
const P_PROG: ParamDescriptor<MaterialCohesive> = law_param!(
    "progressive_delta_f",
    ParamKind::Bool,
    PAR_READ,
    "Whether or not delta_f is equal to delta_max",
    LinearFatigue,
    progressive_delta_f,
    ParamValue::Bool,
    as_bool
);
const P_SWITCHES: ParamDescriptor<MaterialCohesive> = law_param!(
    "count_switches",
    ParamKind::Bool,
    PAR_READ,
    "Count the opening/closing switches per element",
    LinearFatigue,
    count_switches,
    ParamValue::Bool,
    as_bool
);
const P_FRATIO: ParamDescriptor<MaterialCohesive> = law_param!(
    "fatigue_ratio",
    ParamKind::Real,
    PAR_READ,
    "What portion of the cohesive law is subjected to fatigue",
    LinearFatigue,
    fatigue_ratio,
    ParamValue::Real,
    as_real
);
const P_MU: ParamDescriptor<MaterialCohesive> = law_param!(
    "mu",
    ParamKind::Real,
    PAR_READ,
    "Maximum value of the friction coefficient",
    LinearFriction,
    mu_max,
    ParamValue::Real,
    as_real
);
const P_FPEN: ParamDescriptor<MaterialCohesive> = law_param!(
    "penalty_for_friction",
    ParamKind::Real,
    PAR_READ,
    "Penalty parameter for the friction behavior",
    LinearFriction,
    friction_penalty,
    ParamValue::Real,
    as_real
);
const P_ROUGH: ParamDescriptor<MaterialCohesive> = law_param!(
    "roughness",
    ParamKind::Real,
    PAR_READ,
    "Roughness to define coupling between mode II and mode I",
    LinearUncoupled,
    roughness,
    ParamValue::Real,
    as_real
);

/// Upstream `MaterialCohesive`'s own parameters (the base of a runtime
/// law; its runtime parameters follow).
static FOREIGN_PARAMS: [ParamDescriptor<MaterialCohesive>; 2] = [P_SIGMA_C, P_DELTA_C];

static EXPONENTIAL_PARAMS: [ParamDescriptor<MaterialCohesive>; 5] =
    [P_SIGMA_C, P_DELTA_C, P_EXP_BETA, P_EXP_PENALTY, P_EXP_CT];
static FATIGUE_PARAMS: [ParamDescriptor<MaterialCohesive>; 15] = [
    P_SIGMA_C,
    P_DELTA_C,
    P_BETA,
    P_GC,
    P_PENALTY,
    P_VOLUME_S,
    P_M_S,
    P_KAPPA,
    P_CONTACT_AFTER_BREAKING,
    P_MAX_QUAD,
    P_RECOMPUTE,
    P_DELTA_F,
    P_PROG,
    P_SWITCHES,
    P_FRATIO,
];
static FRICTION_PARAMS: [ParamDescriptor<MaterialCohesive>; 13] = [
    P_SIGMA_C,
    P_DELTA_C,
    P_BETA,
    P_GC,
    P_PENALTY,
    P_VOLUME_S,
    P_M_S,
    P_KAPPA,
    P_CONTACT_AFTER_BREAKING,
    P_MAX_QUAD,
    P_RECOMPUTE,
    P_MU,
    P_FPEN,
];
static UNCOUPLED_PARAMS: [ParamDescriptor<MaterialCohesive>; 12] = [
    P_SIGMA_C,
    P_DELTA_C,
    P_BETA,
    P_GC,
    P_PENALTY,
    P_VOLUME_S,
    P_M_S,
    P_KAPPA,
    P_CONTACT_AFTER_BREAKING,
    P_MAX_QUAD,
    P_RECOMPUTE,
    P_ROUGH,
];

/// A cohesive constitutive law.
#[derive(Clone, Debug)]
pub enum CohesiveLaw {
    /// `cohesive_linear`.
    Linear(Linear),
    /// `cohesive_bilinear`.
    Bilinear(Bilinear),
    /// `cohesive_exponential`.
    Exponential(Exponential),
    /// `cohesive_linear_fatigue`.
    LinearFatigue(Fatigue),
    /// `cohesive_linear_friction`.
    LinearFriction(Friction),
    /// `cohesive_linear_uncoupled`.
    LinearUncoupled(Uncoupled),
    /// A runtime-defined law (a Python `MaterialCohesive` subclass,
    /// [`foreign`]).
    Foreign(ForeignCohesive),
}

impl CohesiveLaw {
    /// The law registered as `law_type` (upstream constructors).
    ///
    /// # Errors
    /// Unknown law type.
    pub fn new(law_type: &str, core: &mut CohesiveCore) -> Result<Self> {
        match law_type {
            "cohesive_linear" => Ok(Self::Linear(Linear::new(core)?)),
            "cohesive_bilinear" => Ok(Self::Bilinear(Bilinear {
                linear: Linear::new(core)?,
                delta_0: 0.0,
            })),
            "cohesive_exponential" => Ok(Self::Exponential(Exponential::new(core))),
            "cohesive_linear_fatigue" => Ok(Self::LinearFatigue(Fatigue::new(core)?)),
            "cohesive_linear_friction" => Ok(Self::LinearFriction(Friction::new(core)?)),
            "cohesive_linear_uncoupled" => Ok(Self::LinearUncoupled(Uncoupled::new(core)?)),
            other => Err(Error::not_found("cohesive law", other)),
        }
    }

    /// `true` for the linear-family laws without an elastic branch
    /// (`cohesive_linear`, `_fatigue`, `_friction`, `_uncoupled`): their
    /// per-point strength `σc,eff` (no distribution, default 0) is set only
    /// by the extrinsic insertion check, so an element inserted any other way
    /// has no strength (**UD-224**). `cohesive_bilinear` draws `σc,eff` from
    /// `σc` at initialisation (elastic branch up to `δ₀`) and
    /// `cohesive_exponential` uses `σc` itself.
    #[must_use]
    pub fn strength_from_insertion_check_only(&self) -> bool {
        matches!(
            self,
            Self::Linear(_)
                | Self::LinearFatigue(_)
                | Self::LinearFriction(_)
                | Self::LinearUncoupled(_)
        )
    }

    /// The linear part of linear-family laws.
    #[must_use]
    pub fn linear(&self) -> Option<&Linear> {
        match self {
            Self::Linear(l) => Some(l),
            Self::Bilinear(b) => Some(&b.linear),
            Self::LinearFatigue(f) => Some(&f.linear),
            Self::LinearFriction(f) => Some(&f.linear),
            Self::LinearUncoupled(u) => Some(&u.linear),
            Self::Exponential(_) | Self::Foreign(_) => None,
        }
    }

    /// Mutable [`CohesiveLaw::linear`].
    pub fn linear_mut(&mut self) -> Option<&mut Linear> {
        match self {
            Self::Linear(l) => Some(l),
            Self::Bilinear(b) => Some(&mut b.linear),
            Self::LinearFatigue(f) => Some(&mut f.linear),
            Self::LinearFriction(f) => Some(&mut f.linear),
            Self::LinearUncoupled(u) => Some(&mut u.linear),
            Self::Exponential(_) | Self::Foreign(_) => None,
        }
    }

    pub(crate) fn params(&self) -> &'static [ParamDescriptor<MaterialCohesive>] {
        match self {
            Self::Linear(_) => &LINEAR_PARAMS,
            Self::Bilinear(_) => &BILINEAR_PARAMS,
            Self::Exponential(_) => &EXPONENTIAL_PARAMS,
            Self::LinearFatigue(_) => &FATIGUE_PARAMS,
            Self::LinearFriction(_) => &FRICTION_PARAMS,
            Self::LinearUncoupled(_) => &UNCOUPLED_PARAMS,
            Self::Foreign(_) => &FOREIGN_PARAMS,
        }
    }

    /// Upstream `Math::setTolerance(tol)` as seen by the law.
    pub fn set_tolerance(&mut self, tol: Real) {
        if let Some(l) = self.linear_mut() {
            l.tol = tol;
        }
        if let Self::Exponential(e) = self {
            e.tol = tol;
        }
    }

    pub(crate) fn update_internal_parameters(&mut self) {
        if let Some(l) = self.linear_mut() {
            l.update_internal_parameters();
        }
    }

    /// Domain of a law internal (`None`: not a law internal).
    pub(crate) fn domain_of(&self, name: &str) -> Option<Domain> {
        (self.fields().iter().any(|(n, _)| *n == name)
            || self.bool_fields().iter().any(|(n, _)| *n == name)
            || self.int_fields().iter().any(|(n, _)| *n == name))
        .then_some(Domain::Cohesive)
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, &InternalField<Real>)> {
        let mut v = self.linear().map(Linear::fields).unwrap_or_default();
        match self {
            Self::LinearFatigue(f) => {
                v.extend([
                    ("delta_prec", &f.delta_prec),
                    ("K_plus", &f.k_plus),
                    ("K_minus", &f.k_minus),
                    ("T_1d", &f.t_1d),
                ]);
                if let Some(c) = &f.switch_count {
                    v.push(("delta_dot_prec", &c.delta_dot_prec));
                }
            }
            Self::LinearFriction(f) => v.extend([
                ("residual_sliding", &f.residual_sliding),
                ("friction_force", &f.friction_force),
            ]),
            Self::LinearUncoupled(u) => v.extend([
                ("delta_n_max", &u.delta_n_max),
                ("delta_t_max", &u.delta_t_max),
                ("damage_n", &u.damage_n),
                ("damage_t", &u.damage_t),
            ]),
            _ => {}
        }
        v
    }

    pub(crate) fn fields_mut(&mut self) -> Vec<(&'static str, &mut InternalField<Real>)> {
        match self {
            Self::Linear(l) => l.fields_mut(),
            Self::Bilinear(b) => b.linear.fields_mut(),
            Self::Exponential(_) | Self::Foreign(_) => Vec::new(),
            Self::LinearFatigue(f) => {
                let mut v = f.linear.fields_mut();
                v.extend([
                    ("delta_prec", &mut f.delta_prec),
                    ("K_plus", &mut f.k_plus),
                    ("K_minus", &mut f.k_minus),
                    ("T_1d", &mut f.t_1d),
                ]);
                if let Some(c) = &mut f.switch_count {
                    v.push(("delta_dot_prec", &mut c.delta_dot_prec));
                }
                v
            }
            Self::LinearFriction(f) => {
                let mut v = f.linear.fields_mut();
                v.extend([
                    ("residual_sliding", &mut f.residual_sliding),
                    ("friction_force", &mut f.friction_force),
                ]);
                v
            }
            Self::LinearUncoupled(u) => {
                let mut v = u.linear.fields_mut();
                v.extend([
                    ("delta_n_max", &mut u.delta_n_max),
                    ("delta_t_max", &mut u.delta_t_max),
                    ("damage_n", &mut u.damage_n),
                    ("damage_t", &mut u.damage_t),
                ]);
                v
            }
        }
    }

    /// Boolean internals (`normal_regime` of the fatigue law).
    pub(crate) fn bool_fields(&self) -> Vec<(&'static str, &InternalField<bool>)> {
        match self {
            Self::LinearFatigue(f) => vec![("normal_regime", &f.normal_regime)],
            _ => Vec::new(),
        }
    }

    pub(crate) fn bool_fields_mut(&mut self) -> Vec<(&'static str, &mut InternalField<bool>)> {
        match self {
            Self::LinearFatigue(f) => vec![("normal_regime", &mut f.normal_regime)],
            _ => Vec::new(),
        }
    }

    /// Integer internals (`switches` of the fatigue law with
    /// `count_switches`).
    pub(crate) fn int_fields(&self) -> Vec<(&'static str, &InternalField<Int>)> {
        match self {
            Self::LinearFatigue(f) => f
                .switch_count
                .iter()
                .map(|c| ("switches", &c.switches))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn int_fields_mut(&mut self) -> Vec<(&'static str, &mut InternalField<Int>)> {
        match self {
            Self::LinearFatigue(f) => f
                .switch_count
                .iter_mut()
                .map(|c| ("switches", &mut c.switches))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Names of the maximal-opening internals whose converged values the
    /// traction reads as history and that are committed at the end of each
    /// converged step instead of at the next step entry (**UD-245**):
    /// `delta max` of `cohesive_exponential` and, with the
    /// UD-241 default, of `cohesive_linear` / `cohesive_bilinear`;
    /// `delta_n_max`, `delta_t_max` of `cohesive_linear_uncoupled` (UD-241
    /// default). Empty with `compat.maxima_committed_at_step_entry` and for
    /// the laws that keep upstream's per-evaluation internals
    /// (`cohesive_linear_fatigue`, `cohesive_linear_friction`).
    #[must_use]
    pub fn committed_maxima(&self, core: &CohesiveCore) -> &'static [&'static str] {
        if core.compat.maxima_committed_at_step_entry {
            return &[];
        }
        let converged = !core.compat.delta_max_per_iterate;
        match self {
            Self::Linear(_) | Self::Bilinear(_) if converged && core.delta_max.has_history() => {
                &["delta max"]
            }
            Self::Exponential(_) if core.delta_max.has_history() => &["delta max"],
            Self::LinearUncoupled(_) if converged => &["delta_n_max", "delta_t_max"],
            _ => &[],
        }
    }

    /// After a non-converged step restored the history: the maximal opening
    /// of `cohesive_bilinear` starts at δ₀ (an entry inserted after the last
    /// commit has the history default 0, UD-245).
    pub(crate) fn after_restore(&self, core: &mut CohesiveCore) -> Result<()> {
        if let Self::Bilinear(b) = self {
            let floor = b.delta_0;
            let blocks: Vec<(ElementType, GhostType)> = core
                .delta_max
                .values()
                .as_map()
                .iter_all()
                .map(|(t, g, _)| (t, g))
                .collect();
            for (ty, g) in blocks {
                for v in core.delta_max.values_mut().get_mut(ty, g)?.as_mut_slice() {
                    *v = linear::floored(*v, floor);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn before_init(&mut self, core: &mut CohesiveCore) {
        if let Self::Bilinear(b) = self {
            b.before_init(core);
        }
    }

    pub(crate) fn after_init(
        &mut self,
        core: &mut CohesiveCore,
        base: &MaterialBase,
        ctx: &LawContext<'_>,
    ) -> Result<()> {
        let _ = base;
        match self {
            Self::Linear(l) => l.after_init(core, ctx),
            Self::Bilinear(b) => b.after_init(core, ctx),
            Self::Exponential(e) => {
                e.after_init();
                Ok(())
            }
            // the runtime law's `initMaterial` is its behaviour's
            // (`MaterialCohesive::init_material`)
            Self::Foreign(_) => Ok(()),
            Self::LinearFatigue(f) => {
                f.linear.after_init(core, ctx)?;
                f.after_init()
            }
            Self::LinearFriction(f) => f.linear.after_init(core, ctx),
            Self::LinearUncoupled(u) => u.linear.after_init(core, ctx),
        }
    }

    pub(crate) fn on_elements_added(
        &mut self,
        core: &mut CohesiveCore,
        base: &MaterialBase,
        ctx: &LawContext<'_>,
        elements: &[Element],
    ) -> Result<()> {
        match self {
            Self::Bilinear(b) => b.on_elements_added(core, base, ctx, elements),
            _ => Ok(()),
        }
    }

    /// Upstream `checkInsertion(check_only)`.
    ///
    /// # Errors
    /// Malformed inputs.
    pub fn check_insertion(
        &mut self,
        core: &mut CohesiveCore,
        inputs: &mut InsertionInputs<'_>,
        check_only: bool,
    ) -> Result<()> {
        match self.linear_mut() {
            Some(l) => l.check_insertion(core, inputs, check_only).map(|_| ()),
            // upstream `MaterialCohesive::checkInsertion`: AKANTU_TO_IMPLEMENT
            None => Err(Error::NotImplemented(
                "checkInsertion of cohesive_exponential (extrinsic insertion)",
            )),
        }
    }

    /// [`CohesiveLaw::check_insertion`] (not `check_only`) returning the
    /// facets whose new elements' parameters the check appended, in the
    /// order of the law's arrays.
    pub(crate) fn check_insertion_prepared(
        &mut self,
        core: &mut CohesiveCore,
        inputs: &mut InsertionInputs<'_>,
    ) -> Result<Vec<Element>> {
        match self.linear_mut() {
            Some(l) => l.check_insertion(core, inputs, false),
            None => Err(Error::NotImplemented(
                "checkInsertion of cohesive_exponential (extrinsic insertion)",
            )),
        }
    }

    /// See [`Linear::insertion_marks`] (empty for laws without an
    /// insertion check).
    pub(crate) fn insertion_marks(&self, core: &CohesiveCore) -> Vec<(ElementType, [usize; 4])> {
        self.linear()
            .map(|l| l.insertion_marks(core))
            .unwrap_or_default()
    }

    /// See [`Linear::restore_insertion_marks`].
    pub(crate) fn restore_insertion_marks(
        &mut self,
        core: &mut CohesiveCore,
        marks: &[(ElementType, [usize; 4])],
    ) {
        if let Some(l) = self.linear_mut() {
            l.restore_insertion_marks(core, marks);
        }
    }

    /// The laws' tractions of `(ty, ghost)`; `cohesive_linear` and
    /// `cohesive_bilinear` run their points on the threads of `exec` (each
    /// point from its own data), the other laws serially
    /// (`docs/ARCHITECTURE.md`, "Execution policy").
    pub(crate) fn compute_traction(
        &mut self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        exec: &akantu_core::exec::Exec,
    ) -> Result<()> {
        match self {
            Self::Linear(l) => l.compute_traction(core, ty, ghost, exec),
            Self::Bilinear(b) => b.compute_traction(core, ty, ghost, exec),
            Self::Exponential(e) => e.compute_traction(core, ty, ghost),
            Self::LinearFatigue(f) => f.compute_traction(core, ty, ghost),
            Self::LinearFriction(f) => {
                let explicit = core.default_solver_explicit;
                f.compute_traction(core, ty, ghost, explicit)
            }
            Self::LinearUncoupled(u) => u.compute_traction(core, ty, ghost),
            Self::Foreign(_) => Err(Error::invalid(
                "internal error: the traction of a runtime cohesive law is its behaviour's",
            )),
        }
    }

    pub(crate) fn compute_tangent_traction(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
    ) -> Result<()> {
        match self {
            Self::Linear(l) => l.compute_tangent_traction(core, ty, ghost, tangent),
            // UD-241: the converged maximal opening starts at δ₀
            Self::Bilinear(b) => b.linear.compute_tangent_traction_with(
                core,
                ty,
                ghost,
                tangent,
                Some(b.delta_0),
                Penetration::Relative,
                &mut |_, _| {},
            ),
            // the fatigue traction keeps the per-evaluation update of all its
            // internals (not corrected, UD-241): the tangent reads them as
            // is; its traction's penetration test is `Δn < −tol` (UD-242)
            Self::LinearFatigue(f) => f.linear.compute_tangent_traction_with(
                core,
                ty,
                ghost,
                tangent,
                None,
                Penetration::Absolute,
                &mut |_, _| {},
            ),
            Self::Exponential(e) => e.compute_tangent_traction(core, ty, ghost, tangent),
            Self::LinearFriction(f) => f.compute_tangent_traction(core, ty, ghost, tangent),
            Self::LinearUncoupled(u) => u.compute_tangent_traction(core, ty, ghost, tangent),
            Self::Foreign(_) => Err(Error::invalid(
                "internal error: the tangent of a runtime cohesive law is its behaviour's",
            )),
        }
    }
}
