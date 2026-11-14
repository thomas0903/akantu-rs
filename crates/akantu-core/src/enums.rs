// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_common.hh (ModelType,
// AnalysisMethod, DOFSupportType, NonLinearSolverType, SparseSolverType,
// TimeStepSolverType, IntegrationSchemeType, SolveConvergenceCriteria,
// CohesiveMethod, MatrixType, DetectionType, ContactState, ReduceOperation,
// SynchronizationTag, EventHandlerPriority, SpatialDirection, MeshIOType).

//! Shared enumerations used across solvers, models, I/O and communication.
//!
//! "Class enums" (`AKANTU_CLASS_ENUM_DECLARE`) are parsed from / printed to
//! input files and Python **without** the leading underscore
//! (`newton_raphson`); plain upstream enums use their C++ identifier
//! (`_static`). [`std::fmt::Display`] and [`std::str::FromStr`] follow these
//! conventions **exactly**, as upstream's `operator>>`
//! (`aka_enum_macros.hh:94-118`, an exact `unordered_map` lookup): a class
//! enum rejects `_static`, a plain enum rejects `static`.
//! Upstream enumerator aliases are accepted under their own
//! identifier (`_explicit_lumped_capacity` = `_explicit_lumped_mass` = 2).
//! A rejected value reports upstream's message, `The value <v> is not a
//! valid <Type> valid values are "a", "b", …`, with the values in
//! declaration order (UD-440: upstream lists them in `unordered_map` order
//! and, from the second failure of a process on, starts the list with
//! `", "`).

use std::fmt;
use std::str::FromStr;

use crate::error::Error;

macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident { $( $(#[$vmeta:meta])* $variant:ident => $str:literal ),+ $(,)? }
        $( aliases { $( $alias:literal => $target:ident ),+ $(,)? } )?
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name { $( $(#[$vmeta])* $variant ),+ }

        impl $name {
            /// All variants in upstream order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// Upstream enumerator aliases accepted by `FromStr` (spelling,
            /// variant).
            pub const ALIASES: &'static [(&'static str, $name)] =
                &[$($( ($alias, $name::$target) ),+)?];

            /// Canonical (upstream) spelling.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self { $($name::$variant => $str),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.name())
            }
        }

        impl FromStr for $name {
            type Err = Error;
            /// Exact upstream spelling (or an upstream alias); anything else
            /// is an error with upstream's message (values in declaration
            /// order, UD-440).
            fn from_str(s: &str) -> Result<Self, Error> {
                if let Some(v) = $name::ALL.iter().copied().find(|v| v.name() == s) {
                    return Ok(v);
                }
                if let Some(&(_, v)) = $name::ALIASES.iter().find(|(a, _)| *a == s) {
                    return Ok(v);
                }
                Err(invalid_value(s, stringify!($name), $name::ALL.iter().map(|v| v.name())
                    .chain($name::ALIASES.iter().map(|(a, _)| *a))))
            }
        }
    };
}

/// Upstream's `operator>>` failure message (`aka_enum_macros.hh:104-115`).
fn invalid_value<'a>(value: &str, type_name: &str, valid: impl Iterator<Item = &'a str>) -> Error {
    let list: Vec<String> = valid.map(|v| format!("\"{v}\"")).collect();
    Error::invalid(format!(
        "The value {value} is not a valid {type_name} valid values are {}",
        list.join(", ")
    ))
}

string_enum! {
    /// Physics model kinds (upstream `ModelType`).
    pub enum ModelType {
        /// generic model
        Model => "model",
        /// `SolidMechanicsModel`
        SolidMechanicsModel => "solid_mechanics_model",
        /// `SolidMechanicsModelCohesive`
        SolidMechanicsModelCohesive => "solid_mechanics_model_cohesive",
        /// heat transfer / diffusion
        HeatTransferModel => "heat_transfer_model",
        /// `StructuralMechanicsModel`
        StructuralMechanicsModel => "structural_mechanics_model",
        /// `EmbeddedInterfaceModel`
        EmbeddedModel => "embedded_model",
        /// `ContactMechanicsModel`
        ContactMechanicsModel => "contact_mechanics_model",
        /// `CouplerSolidContact`
        CouplerSolidContact => "coupler_solid_contact",
        /// `CouplerSolidCohesiveContact`
        CouplerSolidCohesiveContact => "coupler_solid_cohesive_contact",
        /// `PhaseFieldModel`
        PhaseFieldModel => "phase_field_model",
        /// `CouplerSolidPhaseField`
        CouplerSolidPhasefield => "coupler_solid_phasefield",
    }
}

string_enum! {
    /// Analysis method (upstream plain enum `AnalysisMethod`).
    ///
    /// Upstream gives `_explicit_lumped_capacity` the same value as
    /// `_explicit_lumped_mass` (2); use [`AnalysisMethod::EXPLICIT_LUMPED_CAPACITY`].
    pub enum AnalysisMethod {
        /// `_static`
        Static => "_static",
        /// `_implicit_dynamic`
        ImplicitDynamic => "_implicit_dynamic",
        /// `_explicit_lumped_mass` (= `_explicit_lumped_capacity`)
        ExplicitLumpedMass => "_explicit_lumped_mass",
        /// `_explicit_consistent_mass`
        ExplicitConsistentMass => "_explicit_consistent_mass",
        /// `_explicit_contact`
        ExplicitContact => "_explicit_contact",
        /// `_implicit_contact`
        ImplicitContact => "_implicit_contact",
    }
    aliases {
        "_explicit_lumped_capacity" => ExplicitLumpedMass,
    }
}

impl AnalysisMethod {
    /// Upstream alias `_explicit_lumped_capacity`.
    pub const EXPLICIT_LUMPED_CAPACITY: AnalysisMethod = AnalysisMethod::ExplicitLumpedMass;
}

string_enum! {
    /// Support of a DOF family (upstream `DOFSupportType`).
    pub enum DofSupportType {
        /// DOFs attached to mesh nodes.
        Nodal => "_dst_nodal",
        /// Generic DOFs (Lagrange multipliers, …).
        Generic => "_dst_generic",
    }
}

string_enum! {
    /// Non-linear solver kinds (upstream `NonLinearSolverType`).
    pub enum NonLinearSolverType {
        /// no non-linear loop
        Linear => "linear",
        /// Newton–Raphson
        NewtonRaphson => "newton_raphson",
        /// Newton–Raphson with initial tangent
        NewtonRaphsonModified => "newton_raphson_modified",
        /// lumped (diagonal) system
        Lumped => "lumped",
        /// Newton–Raphson for contact
        NewtonRaphsonContact => "newton_raphson_contact",
        /// PETSc SNES
        PetscSnes => "petsc_snes",
        /// PETSc TAO
        PetscTao => "petsc_tao",
        /// model default
        Auto => "auto",
    }
}

string_enum! {
    /// Sparse direct solver backends (upstream `SparseSolverType`, plus the
    /// port extension `Faer`; decision records D-014 as amended by D-040).
    pub enum SparseSolverType {
        /// MUMPS (optional native)
        Mumps => "mumps",
        /// Upstream Eigen `SparseLU`: the pure-Rust translation of Eigen
        /// 3.4.0 `SparseLU` + COLAMD (`akantu_linalg::eigen_sparse_lu`),
        /// bit-identical with the strict oracle build on valid finite input.
        Eigen => "eigen",
        /// PETSc KSP (optional native)
        Petsc => "petsc",
        /// default: MUMPS if available, else `eigen` (upstream resolution;
        /// never `faer`)
        Auto => "auto",
        /// Port extension (D-040): the pure-Rust faer sparse LU
        /// (`akantu_linalg::direct::FaerLuSolver`), its own backend
        /// identity, qualified by residual/conditioning criteria, no bitwise
        /// claim; selected explicitly only.
        Faer => "faer",
    }
}

string_enum! {
    /// Time-step solver kinds (upstream `TimeStepSolverType`).
    pub enum TimeStepSolverType {
        /// static
        Static => "static",
        /// dynamic (consistent)
        Dynamic => "dynamic",
        /// dynamic with lumped matrices
        DynamicLumped => "dynamic_lumped",
        /// unset
        NotDefined => "not_defined",
    }
}

string_enum! {
    /// Integration schemes (upstream `IntegrationSchemeType`).
    pub enum IntegrationSchemeType {
        /// pseudo time
        PseudoTime => "pseudo_time",
        /// GeneralizedTrapezoidal(0)
        ForwardEuler => "forward_euler",
        /// GeneralizedTrapezoidal(1/2)
        TrapezoidalRule1 => "trapezoidal_rule_1",
        /// GeneralizedTrapezoidal(1)
        BackwardEuler => "backward_euler",
        /// NewmarkBeta(0, 1/2)
        CentralDifference => "central_difference",
        /// NewmarkBeta(1/6, 1/2)
        FoxGoodwin => "fox_goodwin",
        /// NewmarkBeta(1/2, 1/2)
        TrapezoidalRule2 => "trapezoidal_rule_2",
        /// NewmarkBeta(1/3, 1/2)
        LinearAcceleration => "linear_acceleration",
        /// generic NewmarkBeta(alpha, beta)
        NewmarkBeta => "newmark_beta",
        /// generic GeneralizedTrapezoidal(alpha)
        GeneralizedTrapezoidal => "generalized_trapezoidal",
    }
}

string_enum! {
    /// Convergence criteria (upstream `SolveConvergenceCriteria`).
    pub enum SolveConvergenceCriteria {
        /// residual norm
        Residual => "residual",
        /// solution increment norm
        Solution => "solution",
        /// residual weighted by inverse nodal mass
        ResidualMassWgh => "residual_mass_wgh",
    }
}

string_enum! {
    /// Cohesive insertion method (upstream `CohesiveMethod`).
    pub enum CohesiveMethod {
        /// cohesive elements present from the start
        Intrinsic => "_intrinsic",
        /// dynamic insertion
        Extrinsic => "_extrinsic",
    }
}

string_enum! {
    /// Sparse matrix symmetry (upstream `MatrixType`).
    pub enum MatrixType {
        /// general
        Unsymmetric => "_unsymmetric",
        /// symmetric (lower triangle stored)
        Symmetric => "_symmetric",
        /// unset
        NotDefined => "_mt_not_defined",
    }
}

string_enum! {
    /// Contact detection mode (upstream `DetectionType`).
    pub enum DetectionType {
        /// explicit
        Explicit => "_explicit",
        /// implicit
        Implicit => "_implicit",
    }
}

string_enum! {
    /// Contact state (upstream `ContactState`, values 0/1/2).
    pub enum ContactState {
        /// no contact
        NoContact => "no_contact",
        /// stick
        Stick => "stick",
        /// slip
        Slip => "slip",
    }
}

string_enum! {
    /// Reduction operation for slave→master synchronisation
    /// (upstream `ReduceOperation`).
    pub enum ReduceOperation {
        /// identity (copy)
        Id => "_id",
        /// sum
        Sum => "_sum",
        /// logical or
        Or => "_or",
    }
}

string_enum! {
    /// Spatial directions (upstream `SpatialDirection`).
    pub enum SpatialDirection {
        /// x
        X => "_x",
        /// y
        Y => "_y",
        /// z
        Z => "_z",
    }
}

string_enum! {
    /// Mesh file formats (upstream `MeshIOType`).
    pub enum MeshIoType {
        /// guess from extension
        Auto => "_miot_auto",
        /// Gmsh `.msh`
        Gmsh => "_miot_gmsh",
        /// Gmsh with structural reinterpretation
        GmshStruct => "_miot_gmsh_struct",
        /// TNO Diana
        Diana => "_miot_diana",
        /// Abaqus (declared upstream; see FEATURE_PARITY for reader status)
        Abaqus => "_miot_abaqus",
    }
}

string_enum! {
    /// Synchronisation tags (upstream `SynchronizationTag`, same order).
    pub enum SynchronizationTag {
        /// generic
        Whatever => "whatever",
        /// generic
        Update => "update",
        /// generic
        AskNodes => "ask_nodes",
        /// generic
        Size => "size",
        /// SolidMechanicsModel mass
        SmmMass => "smm_mass",
        /// displacement for grad u
        SmmForGradu => "smm_for_gradu",
        /// boundary/forces/velocities/displacements
        SmmBoundary => "smm_boundary",
        /// velocities and displacements
        SmmUv => "smm_uv",
        /// residual
        SmmRes => "smm_res",
        /// stresses
        SmmStress => "smm_stress",
        /// gradu
        SmmGradu => "smm_gradu",
        /// lumped mass assembly
        SmmDensity => "smm_density",
        /// cohesive facets
        SmmcFacets => "smmc_facets",
        /// cohesive facet connectivity
        SmmcFacetsConn => "smmc_facets_conn",
        /// cohesive facet stress
        SmmcFacetsStress => "smmc_facets_stress",
        /// cohesive damage
        SmmcDamage => "smmc_damage",
        /// GlobalIdsUpdater
        GiuGlobalConn => "giu_global_conn",
        /// CohesiveElementInserter groups
        CeGroups => "ce_groups",
        /// CohesiveElementInserter insertion order
        CeInsertionOrder => "ce_insertion_order",
        /// GroupManager clusters
        GmClusters => "gm_clusters",
        /// diffusion
        Diffusion => "diffusion",
        /// diffusion gradient
        DiffusionGradient => "diffusion_gradient",
        /// level set phi
        HtmPhi => "htm_phi",
        /// level set gradient phi
        HtmGradientPhi => "htm_gradient_phi",
        /// PhaseFieldModel damage
        PfmDamage => "pfm_damage",
        /// CouplerSolidPhaseField damage
        CspDamage => "csp_damage",
        /// CouplerSolidPhaseField strain
        CspStrain => "csp_strain",
        /// non-local averaging
        MnlForAverage => "mnl_for_average",
        /// non-local weights
        MnlWeight => "mnl_weight",
        /// neighbourhood criterion
        NhCriterion => "nh_criterion",
        /// tests
        Test => "test",
        /// user
        User1 => "user_1",
        /// user
        User2 => "user_2",
        /// constitutive-law ids
        ConstitutiveLawId => "constitutive_law_id",
        /// constitutive-law init data
        ClhInitCl => "clh_init_cl",
        /// before dump
        ForDump => "for_dump",
        /// contact nodal data
        CfNodal => "cf_nodal",
        /// contact increment
        CfIncr => "cf_incr",
        /// PETSc solution
        SolverSolution => "solver_solution",
    }
}

/// Relative order of mesh-event handlers (upstream `EventHandlerPriority`);
/// lower runs first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventPriority(pub u8);

impl EventPriority {
    /// `_ehp_highest`
    pub const HIGHEST: Self = Self(0);
    /// `_ehp_mesh`
    pub const MESH: Self = Self(5);
    /// `_ehp_fe_engine`
    pub const FE_ENGINE: Self = Self(9);
    /// `_ehp_synchronizer`
    pub const SYNCHRONIZER: Self = Self(10);
    /// `_ehp_dof_manager`
    pub const DOF_MANAGER: Self = Self(20);
    /// `_ehp_model`
    pub const MODEL: Self = Self(94);
    /// `_ehp_non_local_manager`
    pub const NON_LOCAL_MANAGER: Self = Self(100);
    /// `_ehp_lowest`
    pub const LOWEST: Self = Self(100);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings() {
        assert_eq!(
            NonLinearSolverType::NewtonRaphson.to_string(),
            "newton_raphson"
        );
        assert_eq!(
            "newton_raphson_modified"
                .parse::<NonLinearSolverType>()
                .unwrap(),
            NonLinearSolverType::NewtonRaphsonModified
        );
        assert_eq!(
            "_static".parse::<AnalysisMethod>().unwrap(),
            AnalysisMethod::Static
        );
        // exact upstream spellings only: a plain enum is its
        // identifier, a class enum its bare name
        assert!("static".parse::<AnalysisMethod>().is_err());
        assert!("_newmark_beta".parse::<IntegrationSchemeType>().is_err());
        assert_eq!(
            "newmark_beta".parse::<IntegrationSchemeType>().unwrap(),
            IntegrationSchemeType::NewmarkBeta
        );
        assert!("newton".parse::<NonLinearSolverType>().is_err());
        assert_eq!(SynchronizationTag::ALL.len(), 39);
        assert_eq!(SynchronizationTag::SolverSolution.name(), "solver_solution");
        assert_eq!(
            AnalysisMethod::EXPLICIT_LUMPED_CAPACITY,
            AnalysisMethod::ExplicitLumpedMass
        );
        assert!(EventPriority::FE_ENGINE < EventPriority::DOF_MANAGER);
        assert_eq!(MatrixType::Symmetric.to_string(), "_symmetric");
    }

    /// Checks one enumeration against its upstream table: the variants in
    /// upstream order with their spelling and integer value, both
    /// directions (`Display`, `FromStr`), and the rejection of the other
    /// spelling (`_x` for a class enum, `x` for a plain enum) and of
    /// non-names.
    fn check_table<T>(type_name: &str, all: &[T], table: &[(&str, i32)], value: fn(T) -> i32)
    where
        T: Copy + PartialEq + std::fmt::Debug + fmt::Display + FromStr<Err = Error>,
    {
        assert_eq!(all.len(), table.len(), "{type_name}: number of variants");
        for (&v, &(name, int)) in all.iter().zip(table) {
            assert_eq!(v.to_string(), name, "{type_name}: spelling of {v:?}");
            assert_eq!(value(v), int, "{type_name}: value of {name}");
            assert_eq!(name.parse::<T>().unwrap(), v, "{type_name}: parse {name}");
            let other = match name.strip_prefix('_') {
                Some(bare) => bare.to_string(),
                None => format!("_{name}"),
            };
            assert!(
                other.parse::<T>().is_err(),
                "{type_name}: `{other}` must be rejected (upstream operator>> is exact)"
            );
            for bad in [format!(" {name}"), format!("{name} "), name.to_uppercase()] {
                assert!(bad.parse::<T>().is_err(), "{type_name}: `{bad}` accepted");
            }
        }
        assert!(
            "".parse::<T>().is_err(),
            "{type_name}: empty value accepted"
        );
    }

    #[test]
    fn exhaustive_spelling_table() {
        // upstream lists: aka_common.hh:90-345 (plain enums keep their
        // identifier, AKANTU_CLASS_ENUM_* lists drop the underscore)
        check_table(
            "ModelType",
            ModelType::ALL,
            &[
                ("model", 0),
                ("solid_mechanics_model", 1),
                ("solid_mechanics_model_cohesive", 2),
                ("heat_transfer_model", 3),
                ("structural_mechanics_model", 4),
                ("embedded_model", 5),
                ("contact_mechanics_model", 6),
                ("coupler_solid_contact", 7),
                ("coupler_solid_cohesive_contact", 8),
                ("phase_field_model", 9),
                ("coupler_solid_phasefield", 10),
            ],
            |v| v as i32,
        );
        check_table(
            "AnalysisMethod",
            AnalysisMethod::ALL,
            &[
                ("_static", 0),
                ("_implicit_dynamic", 1),
                ("_explicit_lumped_mass", 2),
                ("_explicit_consistent_mass", 3),
                ("_explicit_contact", 4),
                ("_implicit_contact", 5),
            ],
            |v| v as i32,
        );
        check_table(
            "DofSupportType",
            DofSupportType::ALL,
            &[("_dst_nodal", 0), ("_dst_generic", 1)],
            |v| v as i32,
        );
        check_table(
            "NonLinearSolverType",
            NonLinearSolverType::ALL,
            &[
                ("linear", 0),
                ("newton_raphson", 1),
                ("newton_raphson_modified", 2),
                ("lumped", 3),
                ("newton_raphson_contact", 4),
                ("petsc_snes", 5),
                ("petsc_tao", 6),
                ("auto", 7),
            ],
            |v| v as i32,
        );
        check_table(
            "SparseSolverType",
            SparseSolverType::ALL,
            // `faer`: port extension appended after upstream's list (D-040)
            &[
                ("mumps", 0),
                ("eigen", 1),
                ("petsc", 2),
                ("auto", 3),
                ("faer", 4),
            ],
            |v| v as i32,
        );
        check_table(
            "TimeStepSolverType",
            TimeStepSolverType::ALL,
            &[
                ("static", 0),
                ("dynamic", 1),
                ("dynamic_lumped", 2),
                ("not_defined", 3),
            ],
            |v| v as i32,
        );
        check_table(
            "IntegrationSchemeType",
            IntegrationSchemeType::ALL,
            &[
                ("pseudo_time", 0),
                ("forward_euler", 1),
                ("trapezoidal_rule_1", 2),
                ("backward_euler", 3),
                ("central_difference", 4),
                ("fox_goodwin", 5),
                ("trapezoidal_rule_2", 6),
                ("linear_acceleration", 7),
                ("newmark_beta", 8),
                ("generalized_trapezoidal", 9),
            ],
            |v| v as i32,
        );
        check_table(
            "SolveConvergenceCriteria",
            SolveConvergenceCriteria::ALL,
            &[("residual", 0), ("solution", 1), ("residual_mass_wgh", 2)],
            |v| v as i32,
        );
        check_table(
            "CohesiveMethod",
            CohesiveMethod::ALL,
            &[("_intrinsic", 0), ("_extrinsic", 1)],
            |v| v as i32,
        );
        check_table(
            "MatrixType",
            MatrixType::ALL,
            &[
                ("_unsymmetric", 0),
                ("_symmetric", 1),
                ("_mt_not_defined", 2),
            ],
            |v| v as i32,
        );
        check_table(
            "DetectionType",
            DetectionType::ALL,
            &[("_explicit", 0), ("_implicit", 1)],
            |v| v as i32,
        );
        check_table(
            "ContactState",
            ContactState::ALL,
            &[("no_contact", 0), ("stick", 1), ("slip", 2)],
            |v| v as i32,
        );
        check_table(
            "ReduceOperation",
            ReduceOperation::ALL,
            &[("_id", 0), ("_sum", 1), ("_or", 2)],
            |v| v as i32,
        );
        check_table(
            "SpatialDirection",
            SpatialDirection::ALL,
            &[("_x", 0), ("_y", 1), ("_z", 2)],
            |v| v as i32,
        );
        check_table(
            "MeshIoType",
            MeshIoType::ALL,
            &[
                ("_miot_auto", 0),
                ("_miot_gmsh", 1),
                ("_miot_gmsh_struct", 2),
                ("_miot_diana", 3),
                ("_miot_abaqus", 4),
            ],
            |v| v as i32,
        );
        let tags = [
            "whatever",
            "update",
            "ask_nodes",
            "size",
            "smm_mass",
            "smm_for_gradu",
            "smm_boundary",
            "smm_uv",
            "smm_res",
            "smm_stress",
            "smm_gradu",
            "smm_density",
            "smmc_facets",
            "smmc_facets_conn",
            "smmc_facets_stress",
            "smmc_damage",
            "giu_global_conn",
            "ce_groups",
            "ce_insertion_order",
            "gm_clusters",
            "diffusion",
            "diffusion_gradient",
            "htm_phi",
            "htm_gradient_phi",
            "pfm_damage",
            "csp_damage",
            "csp_strain",
            "mnl_for_average",
            "mnl_weight",
            "nh_criterion",
            "test",
            "user_1",
            "user_2",
            "constitutive_law_id",
            "clh_init_cl",
            "for_dump",
            "cf_nodal",
            "cf_incr",
            "solver_solution",
        ];
        let table: Vec<(&str, i32)> = tags.iter().zip(0..).map(|(&t, i)| (t, i)).collect();
        check_table("SynchronizationTag", SynchronizationTag::ALL, &table, |v| {
            v as i32
        });
    }

    #[test]
    fn upstream_alias_explicit_lumped_capacity() {
        // `_explicit_lumped_capacity = 2` (aka_common.hh:163) is the same
        // enumerator as `_explicit_lumped_mass`
        let v: AnalysisMethod = "_explicit_lumped_capacity".parse().unwrap();
        assert_eq!(v, AnalysisMethod::ExplicitLumpedMass);
        assert_eq!(v, AnalysisMethod::EXPLICIT_LUMPED_CAPACITY);
        assert_eq!(v as i32, 2);
        assert_eq!(v.to_string(), "_explicit_lumped_mass");
        assert!(
            "explicit_lumped_capacity"
                .parse::<AnalysisMethod>()
                .is_err()
        );
        assert!(TimeStepSolverType::ALIASES.is_empty());
    }

    #[test]
    fn rejection_messages_follow_upstream() {
        // aka_enum_macros.hh:104-115: "The value <str> is not a valid <type>
        // valid values are <list>" (list in declaration order here, UD-440)
        let Err(Error::InvalidArgument(m)) = "_static".parse::<TimeStepSolverType>() else {
            panic!("`_static` must be rejected");
        };
        assert_eq!(
            m,
            "The value _static is not a valid TimeStepSolverType valid values are \
             \"static\", \"dynamic\", \"dynamic_lumped\", \"not_defined\""
        );
        let Err(Error::InvalidArgument(m)) = "newton".parse::<NonLinearSolverType>() else {
            panic!("`newton` must be rejected");
        };
        assert_eq!(
            m,
            "The value newton is not a valid NonLinearSolverType valid values are \
             \"linear\", \"newton_raphson\", \"newton_raphson_modified\", \"lumped\", \
             \"newton_raphson_contact\", \"petsc_snes\", \"petsc_tao\", \"auto\""
        );
        // the same message the second time (upstream's static `first` flag
        // would prefix the list with ", ": UD-440, category C)
        let Err(Error::InvalidArgument(again)) = "newton".parse::<NonLinearSolverType>() else {
            panic!("`newton` must be rejected");
        };
        assert_eq!(again, m);
        let Err(Error::InvalidArgument(m)) = "static".parse::<AnalysisMethod>() else {
            panic!("`static` must be rejected");
        };
        assert!(
            m.ends_with("\"_implicit_contact\", \"_explicit_lumped_capacity\""),
            "{m}"
        );
    }
}
