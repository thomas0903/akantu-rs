// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/structural_mechanics/
// structural_mechanics_model.hh, structural_mechanics_model.cc,
// structural_mechanics_model_inline_impl.hh (addMaterial, getMaterial*),
// structural_mechanics_model_boundary.cc (empty upstream).

//! [`StructuralMechanicsModel`] (STRUCT-001…013).
//!
//! # Structure (D-021)
//!
//! `StructuralMechanicsModel { physics, solver, token, dumpers }`: the
//! physics part ([`StructuralMechanicsPhysics`]: model base, structural FE
//! engine, materials, element → material map, generalised stresses,
//! model-owned nodal arrays) implements [`SolverCallback`]; the
//! [`ModelSolver`] owns the DOF manager and the time-step solvers. The DOF
//! family `displacement` holds **displacements and rotations**
//! (`nb_degree_of_freedom` = 3 in 2-D: `u, v, θ`; 6 in 3-D: `u, v, w, θx,
//! θy, θz`), its blocked flags and, for dynamic solvers, its velocity and
//! acceleration (upstream registers neither an increment nor a previous
//! array for this model). `external_force`, `internal_force` and the
//! lumped `mass` (one value per DOF) belong to the physics.
//!
//! # Element kernels
//!
//! Rotations, `N·T`, `B·T`, `det J · w`, `Bᵀσ`, `BᵀDB`, `Nᵀt`, stresses
//! and the consistent element mass come from
//! [`akantu_fe::structural::StructuralFeEngine`] (ARCHITECTURE); the
//! model provides the generalised tangents ([`crate::material`]) and does
//! the global assembly through the DOF manager, in upstream order.
//!
//! # Upstream compatibility (D-003 B, per-model switch as D-041)
//!
//! [`StructuralModelUpstreamCompat`] is fixed at construction: the default
//! is the corrected behaviour (3-D beam θy shape row UD-293, DKT bending
//! rigidity `E t³/(12(1−ν²))` UD-007, 3-D beam torsional inertia
//! `ρ (Iy + Iz)` UD-507, the exact nodal 3×3 rotary block of the lumped
//! beam mass UD-059 / D-059) and refuses inadmissible 3-D beam normals
//! (non-unit or not orthogonal to the axis, UD-343);
//! [`StructuralModelUpstreamCompat::UPSTREAM`] replays the oracle bit for
//! bit (every value the oracle records; upstream's lumped mass is
//! identically zero, UD-005, and stays a documented difference).
//!
//! # Parallelism
//!
//! Upstream's model has no synchronizer and ignores ghost elements (its
//! energies reduce over the communicator, nothing else is distributed),
//! and its structural meshes cannot be partitioned (UD-131). The port is
//! serial: a distributed mesh or a communicator with more than one rank is
//! refused with `Error::Unsupported` at construction (the energies keep
//! upstream's local-or-master filter and the all-reduce, so the hooks a
//! parallel version needs are the element synchronizer tags for the
//! stresses and the ghost-element assembly).

mod assembly;
mod energy;
mod io;
mod loads;

use std::collections::BTreeMap;

use akantu_core::array_shared::{ArrayMut, ArrayRef, SharedArray};
use akantu_core::enums::{
    AnalysisMethod, EventPriority, IntegrationSchemeType, MatrixType, ModelType,
    NonLinearSolverType, TimeStepSolverType,
};
use akantu_core::exec::{Exec, ExecPolicy};
use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Id, Real,
    Result, SharedElementTypeMapArray, TypeFilter, UInt,
};
use akantu_fe::structural::kernels::DktBendingStiffness;
use akantu_fe::structural::{EXTRA_NORMAL, StructuralFeEngine, StructuralUpstreamCompat};
use akantu_mesh::{
    ConsumerToken, Mesh, MeshEvent, MeshEventHandler, MeshEvents, MeshPart, SharedMesh,
};
use akantu_model::model::{
    ModelBase, alloc_nodal_field, alloc_nodal_shared_field, init_new_solver,
};
use akantu_model::{Model, ModelOptions};
use akantu_parser::Parser;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback};

use crate::material::{StructuralMaterial, nb_stress_components};

pub use io::StructuralDumpers;

/// Id of the DOF family of the model (displacements and rotations).
pub const DOF_ID: &str = "displacement";

/// Default model id (upstream `"structural_mechanics_model"`).
pub const DEFAULT_ID: &str = "structural_mechanics_model";

/// Name of the FE engine (upstream `"StructuralMechanicsFEEngine"`).
pub const FE_ENGINE_NAME: &str = "StructuralMechanicsFEEngine";

/// Relative definiteness threshold of the free rotary sub-block of a node
/// in the lumped-mass check (UD-518 extended to the D-059 blocks; input
/// validation of the assembled mass, not a tolerance on results): every
/// `LDLᵀ` pivot of the free sub-block must exceed
/// `ROTARY_BLOCK_DEFINITENESS` times the sub-block's largest diagonal
/// entry. A singular block (e.g. a node whose beams are collinear and have
/// no polar moment, `J_t = 0`: no inertia about their axis) yields a pivot
/// of the order of the rounding of its entries (`≈ 1e-16` relative) with
/// either sign, so the plain `> 0` test would let it through; a regular
/// nodal block has pivots of at least `≈ min(J_t, J_b)/max(J_t, J_b) =
/// min(39·I_p/(A·L²), A·L²/(39·I_p))`, above `1e-12` up to a slenderness
/// `L/r ≈ 4·10⁶` of a single element (`docs/NUMERICAL_VALIDATION.md`).
pub const ROTARY_BLOCK_DEFINITENESS: Real = 1e-12;

/// Upstream-compatibility switches of the structural model (D-003 category
/// B defects; default = corrected behaviour everywhere). Fixed at
/// construction because the FE engine's precomputed shapes depend on
/// [`StructuralModelUpstreamCompat::fe`]. Differential replays against the
/// unpatched oracle use [`StructuralModelUpstreamCompat::UPSTREAM`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StructuralModelUpstreamCompat {
    /// Switches of the structural FE kernels (D-049), fixed for the
    /// engine's lifetime:
    ///
    /// * `beam3_theta_y_legacy` (**UD-293**, D-003 B): θy row of the
    ///   `_bernoulli_beam_3` shape matrix (`N·T`: line loads with a moment
    ///   about the local `y` axis); default corrected `−dw/ds`, `true`
    ///   reproduces upstream's `+dw/ds`;
    /// * `beam3_normal_unchecked` (**UD-343**, D-003 A with a legacy
    ///   switch): by default a `_bernoulli_beam_3` normal (`extra_normal`)
    ///   with `|n·n − 1|` or `|n·t̂|` above
    ///   [`akantu_fe::structural::BEAM3_NORMAL_ADMISSIBILITY`] (`1e-10`,
    ///   input validation, not a tolerance on results) is refused with
    ///   `Error::InvalidArgument` naming the element — upstream builds a
    ///   non-orthogonal frame from it and silently gives wrong stiffness;
    ///   admissible normals are used raw (bitwise as upstream). `true`
    ///   disables the check and reproduces upstream for any finite normal
    ///   (zero, parallel and non-finite normals stay refused, UD-296).
    ///   Applies at `init_full`, [`StructuralMechanicsModel::set_extra_normals`]
    ///   and [`StructuralMechanicsModel::refresh_geometry`].
    ///
    /// [`StructuralUpstreamCompat::UPSTREAM`] (part of
    /// [`StructuralModelUpstreamCompat::UPSTREAM`]) sets both.
    pub fe: StructuralUpstreamCompat,
    /// **UD-007**: bending block of the discrete Kirchhoff triangle tangent
    /// (`Corrected` = `E t³/(12(1−ν²))`, `Legacy` = upstream `E t⁴/…`).
    pub dkt_bending: DktBendingStiffness,
    /// **UD-507**: the consistent mass of `_bernoulli_beam_3` weights its
    /// torsion row (`θx`, the fourth of the `nb_stress` mass rows) with the
    /// mass per unit length `ρ·A` like the translations (upstream
    /// `ComputeRhoFunctorStruct`), while the rotational inertia of the
    /// cross-section about the beam axis is `ρ·(Iy + Iz)` (polar moment of
    /// area). Default (`false`): `ρ·(Iy + Iz)`; `true`: upstream `ρ·A`.
    pub beam3_torsional_mass_rho_a: bool,
    /// **UD-059** (D-059): rotary inertia of the lumped beam mass (the
    /// translational lumped mass is always the corrected `m/2` per node,
    /// UD-005). Default [`LumpedRotaryInertia::Block3x3`], also in
    /// [`StructuralModelUpstreamCompat::UPSTREAM`]: a model option, not a
    /// D-003 switch (upstream's lumped values are identically zero and
    /// cannot be reproduced; D-059 amendment).
    pub lumped_rotary_inertia: LumpedRotaryInertia,
}

impl StructuralModelUpstreamCompat {
    /// Every upstream behaviour (oracle replays).
    pub const UPSTREAM: Self = Self {
        fe: StructuralUpstreamCompat::UPSTREAM,
        dkt_bending: DktBendingStiffness::Legacy,
        beam3_torsional_mass_rho_a: true,
        lumped_rotary_inertia: LumpedRotaryInertia::Block3x3,
    };
}

/// Rotary inertia of the lumped mass of Bernoulli beams (UD-059, D-059;
/// [`StructuralMechanicsModel::assemble_lumped_mass_matrix`]).
///
/// The element-frame coefficients are the Hinton–Rock–Zienkiewicz
/// scalings of the consistent beam mass: bending `J_b = m·L²/78` per node
/// and bending rotation (`m = ρ·A·L`), torsion `J_t = ρ·(I_y + I_z)·L/2`
/// per node. In 2-D, [`LumpedRotaryInertia::Block3x3`] and
/// [`LumpedRotaryInertia::IsotropicMax`] both put `J_b` on the rotation of
/// each node (one rotation: nothing to couple).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LumpedRotaryInertia {
    /// **Corrected default (D-059).** 3-D: the exact nodal 3×3 block
    /// `Jₙ = Σ_e Tₑᵀ·diag(J_t, J_b, J_b)·Tₑ` on the three rotations of each
    /// node (`Tₑ` the element's global → member rotation, the frame of its
    /// stiffness and consistent mass): objective, exact HRZ in every member
    /// frame. The lumped matrix carries the off-diagonal entries as block
    /// couplings of the DOF manager (block product in the residual, block
    /// solve of the free sub-block in the explicit lumped solve);
    /// `lumped_mass()` holds the diagonal of the blocks and
    /// `rotary_inertia()` the full blocks. For every admissible frame (the
    /// normal orthogonal to the axis) the block equals the axis-only form
    /// `J_t·aaᵀ + J_b·(I − aaᵀ)` (`a` the unit axis). Under the UD-343
    /// compat (`fe.beam3_normal_unchecked`) with a normal that is not
    /// orthogonal to the axis, `Tₑ` is not orthogonal and the block moves
    /// away from that form (29 % of `J_b` in the case
    /// `v127_review_checks.rs::v127_block_with_a_skew_normal_under_ud343_
    /// compat`): the combination reproduces no upstream result (upstream's
    /// lumped mass has no rotary inertia) and every fixture has admissible
    /// normals.
    #[default]
    Block3x3,
    /// An interim rule:
    /// each element adds the isotropic `max(J_t, J_b)` to all three
    /// rotations of both its nodes. Diagonal and objective, for users who
    /// need a diagonal mass; torsional frequencies low by `≈ √(J_t/J_b)`
    /// when `J_b > J_t` (elements longer than `√(39·I_p/A)`, `4.42·r` for a
    /// circular section; UD-059).
    IsotropicMax,
}

/// Physics part of the structural model: everything except the solver.
/// Implements [`SolverCallback`].
#[derive(Debug)]
pub struct StructuralMechanicsPhysics {
    /// Model base (mesh handle, id, spatial dimension, method, parser). Its
    /// Lagrange FE-engine registry is unused: the structural engine is
    /// [`StructuralMechanicsPhysics::fe_engine`].
    pub base: ModelBase,
    fe: StructuralFeEngine,
    compat: StructuralModelUpstreamCompat,
    nb_degree_of_freedom: usize,
    materials: Vec<StructuralMaterial>,
    material_names: BTreeMap<Id, usize>,
    /// Exportable (D-047; zero-copy writable views of
    /// `getElementMaterial`).
    element_material: SharedElementTypeMapArray<UInt>,
    set_id: ElementTypeMapArray<UInt>,
    stress: ElementTypeMapArray<Real>,
    /// Exportable (D-047; zero-copy writable views of
    /// `getExternalForce` / `getInternalForce`).
    external_force: Option<SharedArray<Real>>,
    internal_force: Option<SharedArray<Real>>,
    mass: Option<Array<Real>>,
    /// Nodal rotary inertia of the lumped mass (UD-059, D-059): `nr × nr`
    /// values per node, row-major (`nr` = 1 rotation in 2-D, 3 in 3-D).
    rotary_inertia: Option<Array<Real>>,
    /// Inputs of the last stiffness / consistent-mass / lumped-mass
    /// assembly (`None`: reassemble). Upstream assembles each once (UD-508);
    /// the port reassembles when an input changed, whoever wrote it
    /// (the Python layer hands out zero-copy views, D-047).
    assembled_stiffness: Option<AssemblyInputs>,
    assembled_mass: Option<AssemblyInputs>,
    assembled_lumped_mass: Option<AssemblyInputs>,
    /// Upstream `f_m2a` (kept for API parity; unused by upstream code).
    pub f_m2a: Real,
    time_step: Real,
}

/// The inputs of an assembly besides the mesh (whose node coordinates and
/// `extra_normal` rows the FE engine's stamps cover): the bit patterns of
/// every material parameter and the element → material map of the
/// assembled types. Compared exactly (bitwise) with the inputs of the last
/// assembly, `O(n_materials + n_elements)` per call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssemblyInputs {
    materials: Vec<[u64; 9]>,
    element_material: Vec<(ElementType, Vec<UInt>)>,
}

impl AssemblyInputs {
    pub(crate) fn of(
        materials: &[StructuralMaterial],
        element_material: &SharedElementTypeMapArray<UInt>,
        types: &[ElementType],
    ) -> Self {
        Self {
            materials: materials
                .iter()
                .map(|m| [m.e, m.a, m.i, m.iz, m.iy, m.gj, m.rho, m.t, m.nu].map(f64::to_bits))
                .collect(),
            element_material: types
                .iter()
                .map(|&ty| {
                    let map = element_material
                        .get(ty, GhostType::NotGhost)
                        .map(|a| a.as_slice().to_vec())
                        .unwrap_or_default();
                    (ty, map)
                })
                .collect(),
        }
    }
}

/// Structural mechanics model (upstream `StructuralMechanicsModel`).
#[derive(Debug)]
pub struct StructuralMechanicsModel {
    /// Physics (callback).
    pub physics: StructuralMechanicsPhysics,
    /// Solver.
    pub solver: ModelSolver,
    /// Mesh-consumer registration.
    pub token: ConsumerToken,
    /// Dumpers of the model (upstream: registered on the mesh).
    pub dumpers: StructuralDumpers,
}

/// Shape check of a nodal array read by a model loop: the public accessors
/// hand out `&mut Array`, so every loop validates `nb_nodes` tuples of
/// `nb_component` values first (no panic on a user-resized array).
pub(crate) fn check_nodal<T>(
    context: &'static str,
    a: &Array<T>,
    nb_nodes: usize,
    nb_component: usize,
) -> Result<()> {
    if a.len() != nb_nodes {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_nodes,
            got: a.len(),
        });
    }
    if a.nb_component() != nb_component {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_component,
            got: a.nb_component(),
        });
    }
    Ok(())
}

pub(crate) fn not_allocated(name: &str) -> Error {
    Error::not_found(
        "nodal field",
        format!("{name} (not allocated: call init_full first)"),
    )
}

/// Structural element types of `mesh` of every dimension (`ghost` block).
pub(crate) fn structural_types(mesh: &Mesh, ghost: GhostType) -> Vec<ElementType> {
    mesh.element_types(TypeFilter {
        spatial_dimension: None,
        ghost_type: ghost,
        kind: Some(ElementKind::Structural),
    })
    .collect()
}

/// Upstream DOFs per node of a structural model of dimension `dim`.
fn dofs_per_node(dim: usize) -> Result<usize> {
    match dim {
        2 => Ok(3),
        3 => Ok(6),
        // upstream: AKANTU_TO_IMPLEMENT()
        _ => Err(Error::NotImplemented(
            "StructuralMechanicsModel in 1-D (upstream AKANTU_TO_IMPLEMENT: 3 DOFs per node in \
             2-D, 6 in 3-D)",
        )),
    }
}

/// Whether the `extra_normal` data holds a block of `nb_element` tuples of
/// `dim` values for every structural `(type, ghost)` of the mesh.
fn extra_normal_conforms(mesh: &Mesh, dim: usize) -> bool {
    let Ok(map) = mesh.data().reals(EXTRA_NORMAL) else {
        return false;
    };
    GhostType::ALL.into_iter().all(|g| {
        structural_types(mesh, g).into_iter().all(|ty| {
            map.get(ty, g)
                .is_ok_and(|a| a.nb_component() == dim && a.len() == mesh.nb_element(ty, g))
        })
    })
}

/// Upstream constructor's `mesh.getElementalData<Real>("extra_normal")
/// .initialize(mesh, _element_kind = _ek_structural, _nb_component = dim,
/// _with_nb_element = true, _default_value = 0.)`: existing rows are kept,
/// new rows are zero.
fn initialize_extra_normal(mesh: &mut Mesh, dim: usize) -> Result<()> {
    let blocks: Vec<(ElementType, GhostType, usize)> = GhostType::ALL
        .into_iter()
        .flat_map(|g| {
            structural_types(mesh, g)
                .into_iter()
                .map(move |ty| (ty, g))
                .collect::<Vec<_>>()
        })
        .map(|(ty, g)| (ty, g, mesh.nb_element(ty, g)))
        .collect();
    let map = mesh.data_mut().reals_mut_or_insert(EXTRA_NORMAL)?;
    for (ty, g, n) in blocks {
        map.alloc(n, dim, ty, g, 0.0)?;
    }
    Ok(())
}

/// Validate new `_bernoulli_beam_3` normals (`3` values per element, not
/// ghost) with the element kernel's frame construction and `compat`
/// (UD-296 always; UD-343 unless `beam3_normal_unchecked`), before any
/// state change.
fn validate_beam3_normals(
    mesh: &Mesh,
    normals: &[Real],
    compat: StructuralUpstreamCompat,
) -> Result<()> {
    use akantu_fe::element_class::types::BernoulliBeam3;
    use akantu_fe::structural::{Coords, StructuralElement};
    let ty = ElementType::BernoulliBeam3;
    let conn = mesh.connectivity(ty, GhostType::NotGhost)?;
    let x_g = mesh.nodes();
    let x = x_g.as_slice();
    let mut coords = [0.0; 6];
    let mut r = [0.0; 36];
    for e in 0..conn.len() {
        for (k, &node) in conn.tuple(e).iter().enumerate() {
            let n = akantu_core::try_to_usize(node)?;
            let xn = x.get(n * 3..n * 3 + 3).ok_or(Error::OutOfRange {
                what: "node of a beam",
                index: n,
                len: mesh.nb_nodes(),
            })?;
            coords[k * 3..k * 3 + 3].copy_from_slice(xn);
        }
        let normal = normals
            .get(e * 3..e * 3 + 3)
            .ok_or(Error::DimensionMismatch {
                context: "set_extra_normals: values (3 per element)",
                expected: conn.len() * 3,
                got: normals.len(),
            })?;
        <BernoulliBeam3 as StructuralElement>::compute_rotation_matrix_with(
            Coords::new(&coords, 3, 2)?,
            normal,
            compat,
            &mut r,
        )
        .map_err(|err| match err {
            Error::InvalidArgument(m) => Error::InvalidArgument(format!(
                "{m} (element {e} of {}: set_extra_normals refused, mesh unchanged)",
                ty.name()
            )),
            Error::Numerical(m) => Error::Numerical(format!(
                "{m} (element {e} of {}: set_extra_normals refused, mesh unchanged)",
                ty.name()
            )),
            other => other,
        })?;
    }
    Ok(())
}

/// Handler used for data-only mutations of the mesh (no event is recorded,
/// so nothing is dispatched).
#[derive(Debug)]
struct DataOnly;

impl MeshEventHandler for DataOnly {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }
    /// The model's registered levels (the mutation preflight checks them).
    fn event_priorities(&self) -> Vec<EventPriority> {
        vec![
            EventPriority::FE_ENGINE,
            EventPriority::DOF_MANAGER,
            EventPriority::MODEL,
        ]
    }
    fn on_mesh_event(&mut self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        Err(data_only_refusal())
    }
    /// Refuses every event before the commit, with the error of its
    /// dispatch: a data-only mutation records no event,
    /// so one that does is rolled back recoverably instead of poisoning the
    /// mesh domain at dispatch.
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        Err(data_only_refusal())
    }
}

/// The refusal of [`DataOnly`] (capability check and dispatch).
fn data_only_refusal() -> Error {
    Error::invalid("a data-only mesh mutation of the structural model recorded a topology event")
}

impl StructuralMechanicsModel {
    /// Upstream `StructuralMechanicsModel(mesh, dim, id)` with an empty
    /// parser and the corrected default behaviour.
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::with_options`].
    pub fn new(mesh: SharedMesh, spatial_dimension: Option<usize>, id: &str) -> Result<Self> {
        Self::with_options(
            mesh,
            spatial_dimension,
            id,
            Parser::default(),
            StructuralModelUpstreamCompat::default(),
        )
    }

    /// [`StructuralMechanicsModel::new`] with explicit upstream-compatibility
    /// switches ([`StructuralModelUpstreamCompat::UPSTREAM`] for oracle
    /// replays).
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::with_options`].
    pub fn with_compat(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        compat: StructuralModelUpstreamCompat,
    ) -> Result<Self> {
        Self::with_options(mesh, spatial_dimension, id, Parser::default(), compat)
    }

    /// Upstream constructor (STRUCT-001) with an explicit input-file
    /// snapshot (D-023) and compatibility switches: DOFs per node 3 (2-D)
    /// or 6 (3-D); structural FE engine `StructuralMechanicsFEEngine` on the
    /// structural elements of the model dimension (shapes computed by
    /// `init_full`, as upstream's `initModel`); the elemental mesh data
    /// `extra_normal` (`dim` values per structural element, zero for new
    /// rows, existing rows kept); DOF manager `<id>:dof_manager_default`
    /// with the solvers of the model's `model structural_mechanics_model
    /// [ … ]` section; ParaView dumper `structural_mechanics_model` on the
    /// structural elements.
    ///
    /// # Errors
    /// * a spatial dimension other than the mesh's, or 1
    ///   (`Error::Unsupported` / `Error::NotImplemented`);
    /// * a distributed mesh or a communicator with more than one rank
    ///   (`Error::Unsupported`: the model is serial, see the module docs);
    /// * structural elements whose spatial dimension differs from the
    ///   model's (UD-511, `Error::Unsupported`);
    /// * `extra_normal` data that must be created while other consumers are
    ///   registered on the mesh (create the model before them, or provide
    ///   the data);
    /// * an unusable mesh domain or an invalid model section.
    pub fn with_options(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
        compat: StructuralModelUpstreamCompat,
    ) -> Result<Self> {
        let mut base = ModelBase::new(
            mesh.clone(),
            ModelType::StructuralMechanicsModel,
            spatial_dimension,
            id,
        )?;
        base.parser = parser;
        let dim = base.spatial_dimension;
        let nb_degree_of_freedom = dofs_per_node(dim)?;
        let (needs_normals, comm) = {
            let m = mesh.read()?;
            if m.spatial_dimension() != dim {
                return Err(Error::Unsupported(format!(
                    "a {dim}-D structural model on a {}-D mesh (the structural elements' \
                     spatial dimension is the mesh dimension)",
                    m.spatial_dimension()
                )));
            }
            if m.is_distributed() || m.communicator().size() > 1 {
                return Err(Error::Unsupported(format!(
                    "StructuralMechanicsModel is serial (upstream has no synchronizer and \
                     ignores ghost elements; structural meshes cannot be partitioned, UD-131): \
                     the mesh is {} on {} rank(s)",
                    if m.is_distributed() {
                        "distributed"
                    } else {
                        "replicated"
                    },
                    m.communicator().size()
                )));
            }
            for g in GhostType::ALL {
                if let Some(ty) = structural_types(&m, g)
                    .into_iter()
                    .find(|ty| ty.spatial_dimension() != dim)
                {
                    return Err(Error::Unsupported(format!(
                        "structural elements {} (spatial dimension {}) in a {dim}-D structural \
                         model: upstream assembles the stiffness and the stresses of the model \
                         dimension only but the internal forces of every dimension (UD-511)",
                        ty.name(),
                        ty.spatial_dimension()
                    )));
                }
            }
            (!extra_normal_conforms(&m, dim), m.communicator().clone())
        };
        if needs_normals {
            mesh.setup(|m| {
                initialize_extra_normal(m, dim)?;
                Ok(MeshEvents::default())
            })
            .map_err(|e| {
                Error::Mesh(format!(
                    "StructuralMechanicsModel: cannot create the \"{EXTRA_NORMAL}\" mesh data \
                     ({e}); create the model before any other mesh consumer or provide \
                     \"{EXTRA_NORMAL}\" ({dim} values per structural element) beforehand"
                ))
            })?;
        }
        let token = mesh.register_multilevel_consumer(
            id,
            &[
                EventPriority::FE_ENGINE,
                EventPriority::DOF_MANAGER,
                EventPriority::MODEL,
            ],
            MeshPart::Main,
        )?;
        let fe = {
            let m = mesh.read()?;
            StructuralFeEngine::new_with_compat(
                &m,
                Some(dim),
                format!("{id}:fem:{FE_ENGINE_NAME}"),
                compat.fe,
            )?
        };
        let mut physics = StructuralMechanicsPhysics {
            base,
            fe,
            compat,
            nb_degree_of_freedom,
            materials: Vec::new(),
            material_names: BTreeMap::new(),
            element_material: SharedElementTypeMapArray::new(format!("{id}:element_material")),
            set_id: ElementTypeMapArray::new(format!("{id}:beam sets")),
            stress: ElementTypeMapArray::new(format!("{id}:stress")),
            external_force: None,
            internal_force: None,
            mass: None,
            rotary_inertia: None,
            assembled_stiffness: None,
            assembled_mass: None,
            assembled_lumped_mass: None,
            f_m2a: 1.0,
            time_step: 0.0,
        };
        // upstream initDOFManager() (`structural_mechanics_model.cc:78`,
        // `model_solver.cc:78-95`): the section's dof_manager_type through
        // the DOF-manager factory (`default`; `petsc` only in a build with
        // akantu-solver's opt-in `petsc` feature, GPL-3.0-or-later as a
        // whole, D-076) and the model section's solvers/schemes
        let section = physics.base.parser_section().cloned();
        let ty = match &section {
            Some(s) => s
                .get_parameter("dof_manager_type", akantu_parser::SearchScope::Current)
                .map(|p| p.to_word().to_string_lossy().into_owned())
                .unwrap_or_else(|_| "default".to_owned()),
            None => "default".to_owned(),
        };
        let dof_manager = akantu_solver::model_solver::new_dof_manager(&ty, id, &comm)?;
        let mut solver = ModelSolver::new(dof_manager);
        if let Some(section) = section {
            // upstream's global `Parser::isPermissive()` for the
            // `non_linear_solver` sub-section
            let permissive = physics.base.parser.is_permissive();
            solver.init_from_section(&section, &mut physics, permissive)?;
        }
        // the process default execution policy (D-123): the matrix
        // assembly and the DOF manager's loops
        solver
            .dof_manager
            .set_exec(Exec::new(ExecPolicy::process_default()?)?);
        let dumpers = StructuralDumpers::new(id, dim)?;
        physics.time_step = solver.time_step(None).unwrap_or(0.0);
        Ok(Self {
            physics,
            solver,
            token,
            dumpers,
        })
    }

    /// The execution policy of the model's kernels (D-123): precision
    /// mode, threads, SIMD level. A new model takes
    /// [`ExecPolicy::process_default`].
    #[must_use]
    pub fn exec_policy(&self) -> ExecPolicy {
        self.solver.dof_manager.exec().policy()
    }

    /// Set the execution policy of the model's kernels: its matrix
    /// assembly and the matrix operations and
    /// integration-scheme loops of its DOF manager. May be called at any
    /// time between two steps. With
    /// [`akantu_core::exec::Precision::Bitwise`] every result is bitwise
    /// the same for every thread count and SIMD level.
    ///
    /// # Errors
    /// The operating system refuses the threads.
    pub fn set_exec_policy(&mut self, policy: ExecPolicy) -> Result<()> {
        let exec = Exec::new(policy)?;
        self.solver.dof_manager.set_exec(exec);
        Ok(())
    }

    /// Upstream `initFull(options)` (STRUCT-001, MODEL-002): method; the
    /// default solver of the method if none exists (`initNewSolver`:
    /// `initSolver` allocates the nodal fields and registers the DOFs,
    /// the integration schemes create their matrices); `initModel`
    /// (element → material map of every structural type, default `0`;
    /// shapes of both ghost types); generalised stresses (`nb_stress`
    /// components per quadrature point).
    ///
    /// # Errors
    /// An analysis method the model does not know (upstream prints
    /// `"UNKOWN."` and fails in `getDefaultSolverOptions`, UD-509);
    /// solver creation errors; FE precomputation errors (missing or
    /// degenerate `extra_normal` of 3-D beams, degenerate geometry).
    pub fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        let method = options.analysis_method;
        self.physics.base.method = method;
        if !self.solver.has_default_solver() {
            let id = self.physics.default_solver_id(method);
            if id.1 == TimeStepSolverType::NotDefined {
                return Err(Error::invalid(format!(
                    "analysis method {method} is not known by the structural mechanics model \
                     (static, implicit_dynamic, explicit_lumped_mass, explicit_consistent_mass)"
                )));
            }
            init_new_solver(
                &mut self.solver,
                &mut self.physics,
                id,
                options.solver_options.clone(),
            )?;
            self.physics.base.method = method;
        }
        self.init_model()?;
        self.physics.init_stresses()?;
        self.physics.time_step = self.solver.time_step(None).unwrap_or(0.0);
        Ok(())
    }

    /// Upstream `initFull(_analysis_method = method)`.
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::init_full`].
    pub fn init_full_with(&mut self, method: AnalysisMethod) -> Result<()> {
        self.init_full(&ModelOptions::new(method))
    }

    /// Upstream `initModel()`: `element_material` of every structural type
    /// (all dimensions, both ghost types, one value per element, default
    /// `0`, existing values kept) and the FE shapes of both ghost types.
    fn init_model(&mut self) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        for g in GhostType::ALL {
            for ty in structural_types(&mesh, g) {
                self.physics
                    .element_material
                    .alloc(mesh.nb_element(ty, g), 1, ty, g, 0)?;
            }
        }
        self.physics
            .fe
            .init_shape_functions(&mesh, GhostType::NotGhost)?;
        self.physics
            .fe
            .init_shape_functions(&mesh, GhostType::Ghost)?;
        self.physics.invalidate_matrices();
        Ok(())
    }

    /// Solve one step (upstream `solveStep(solver_id)`): checks that the
    /// model processed every mesh change, then the solver drives the physics.
    ///
    /// # Errors
    /// Solver and assembly errors (see the callbacks).
    pub fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        self.physics.base.mesh.check_current(&self.token)?;
        self.physics.time_step = self.solver.time_step(solver_id)?;
        self.solver.solve_step(&mut self.physics, solver_id)
    }

    /// Solve one step with an intercepting user callback (upstream
    /// `solveStep(InterceptSolverCallback &, solver_id)`).
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::solve_step`].
    pub fn solve_step_intercepted(
        &mut self,
        user: &mut dyn akantu_solver::callback::InterceptCallback,
        solver_id: Option<&str>,
    ) -> Result<()> {
        self.physics.base.mesh.check_current(&self.token)?;
        self.physics.time_step = self.solver.time_step(solver_id)?;
        self.solver
            .solve_step_intercepted(&mut self.physics, user, solver_id)
    }

    /// Upstream `setTimeStep(dt, solver_id)`: the solver's time step and the
    /// default dumper's (`structural_mechanics_model.cc:130-134`; other
    /// dumpers keep theirs).
    ///
    /// # Errors
    /// An unknown solver.
    pub fn set_time_step(&mut self, time_step: Real, solver_id: Option<&str>) -> Result<()> {
        self.solver.set_time_step(time_step, solver_id)?;
        self.physics.time_step = self.solver.time_step(None).unwrap_or(time_step);
        self.dumpers.set_time_step(time_step)
    }

    /// Upstream `getTimeStep(solver_id)`.
    ///
    /// # Errors
    /// An unknown solver.
    pub fn time_step(&self, solver_id: Option<&str>) -> Result<Real> {
        self.solver.time_step(solver_id)
    }

    /// Upstream `getNewSolver(id, options)` (the model's `init_solver` runs).
    ///
    /// # Errors
    /// Solver creation errors.
    pub fn get_new_solver(&mut self, solver_id: &str, options: &ModelSolverOptions) -> Result<()> {
        self.solver
            .get_new_solver(solver_id, options, &mut self.physics)
    }

    /// Upstream `setIntegrationScheme(solver_id, dof_id, type, solution_type)`.
    ///
    /// # Errors
    /// An unknown solver, a scheme already set.
    pub fn set_integration_scheme(
        &mut self,
        solver_id: &str,
        dof_id: &str,
        ty: IntegrationSchemeType,
        solution_type: SolutionType,
    ) -> Result<()> {
        self.solver
            .set_integration_scheme(solver_id, dof_id, ty, solution_type, &self.physics)
    }

    /// Upstream `initNewSolver(method)`: the method's default solver
    /// (created with the model's default options if missing) becomes the
    /// default solver. The method is set first, as the port does for the
    /// solid model (UD-212: upstream sets it after creating the solver, so
    /// `getDefaultSolverOptions` reads the previous method).
    ///
    /// # Errors
    /// An unknown analysis method; solver creation errors.
    pub fn init_new_solver(&mut self, method: AnalysisMethod) -> Result<()> {
        let id = self.physics.default_solver_id(method);
        if id.1 == TimeStepSolverType::NotDefined {
            return Err(Error::invalid(format!(
                "analysis method {method} is not known by the structural mechanics model"
            )));
        }
        self.physics.base.method = method;
        init_new_solver(
            &mut self.solver,
            &mut self.physics,
            id,
            ModelSolverOptions::default(),
        )?;
        self.physics.base.method = method;
        self.physics.time_step = self.solver.time_step(None).unwrap_or(0.0);
        Ok(())
    }

    /// The non-linear solver of a time-step solver (upstream
    /// `getNonLinearSolver(solver_id)`), e.g. to `set_param("threshold", …)`.
    ///
    /// # Errors
    /// An unknown solver.
    pub fn non_linear_solver_mut(
        &mut self,
        solver_id: Option<&str>,
    ) -> Result<&mut dyn akantu_solver::NonLinearSolver> {
        self.solver.non_linear_solver_mut(solver_id)
    }

    /// The DOF manager (upstream `getDOFManager()`).
    #[must_use]
    pub fn dof_manager(&self) -> &DofManager {
        &self.solver.dof_manager
    }

    /// Mutable DOF manager.
    pub fn dof_manager_mut(&mut self) -> &mut DofManager {
        &mut self.solver.dof_manager
    }

    /// The structural FE engine (upstream `getFEEngine()`).
    #[must_use]
    pub fn fe_engine(&self) -> &StructuralFeEngine {
        &self.physics.fe
    }

    /// Upstream-compatibility switches of the model.
    #[must_use]
    pub fn upstream_compat(&self) -> StructuralModelUpstreamCompat {
        self.physics.compat
    }

    /// DOFs per node (upstream `nb_degree_of_freedom`: 3 in 2-D, 6 in 3-D).
    #[must_use]
    pub fn nb_degree_of_freedom(&self) -> usize {
        self.physics.nb_degree_of_freedom
    }

    /// Analysis method.
    #[must_use]
    pub fn analysis_method(&self) -> AnalysisMethod {
        self.physics.base.method
    }

    /// Spatial dimension.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.physics.base.spatial_dimension
    }

    /// Model id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.physics.base.id
    }

    // ---- nodal fields --------------------------------------------------

    fn family(&self) -> Result<&akantu_solver::dof_manager::DofData> {
        self.solver
            .dof_manager
            .family(DOF_ID)
            .map_err(|_| not_allocated("displacement"))
    }

    fn family_mut(&mut self) -> Result<&mut akantu_solver::dof_manager::DofData> {
        self.solver
            .dof_manager
            .family_mut(DOF_ID)
            .map_err(|_| not_allocated("displacement"))
    }

    /// Upstream `getDisplacement()`: displacements **and rotations**
    /// (`nb_degree_of_freedom` components per node).
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn displacement(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?.dofs.read()
    }

    /// Mutable displacements and rotations.
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn displacement_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.family_mut()?.dofs.write()
    }

    /// Upstream `getVelocity()` (dynamic analyses only).
    ///
    /// # Errors
    /// Not allocated (static analyses).
    pub fn velocity(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?
            .derivatives
            .get(&1)
            .ok_or_else(|| not_allocated("velocity"))?
            .read()
    }

    /// Mutable velocity.
    ///
    /// # Errors
    /// Not allocated (static analyses).
    pub fn velocity_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.family_mut()?
            .derivatives
            .get_mut(&1)
            .ok_or_else(|| not_allocated("velocity"))?
            .write()
    }

    /// Upstream `getAcceleration()` (dynamic analyses only).
    ///
    /// # Errors
    /// Not allocated (static analyses).
    pub fn acceleration(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?
            .derivatives
            .get(&2)
            .ok_or_else(|| not_allocated("acceleration"))?
            .read()
    }

    /// Mutable acceleration.
    ///
    /// # Errors
    /// Not allocated (static analyses).
    pub fn acceleration_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.family_mut()?
            .derivatives
            .get_mut(&2)
            .ok_or_else(|| not_allocated("acceleration"))?
            .write()
    }

    /// Upstream `getBlockedDOFs()` (boundary conditions: users write the
    /// flags and the imposed values directly, as upstream — the structural
    /// model is not a `BoundaryCondition`).
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn blocked_dofs(&self) -> Result<ArrayRef<'_, bool>> {
        self.family()?
            .blocked
            .as_ref()
            .ok_or_else(|| not_allocated("blocked_dofs"))?
            .read()
    }

    /// Mutable blocked flags.
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn blocked_dofs_mut(&mut self) -> Result<ArrayMut<'_, bool>> {
        self.family_mut()?
            .blocked
            .as_mut()
            .ok_or_else(|| not_allocated("blocked_dofs"))?
            .write()
    }

    /// Upstream `getExternalForce()` (forces and moments).
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn external_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.physics
            .external_force
            .as_ref()
            .ok_or_else(|| not_allocated("external_force"))?
            .read()
    }

    /// Mutable external forces and moments.
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn external_force_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.external_force_shared_mut()?.write()
    }

    /// The exportable external forces (zero-copy views for bindings,
    /// D-047).
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn external_force_shared_mut(&mut self) -> Result<&mut SharedArray<Real>> {
        self.physics
            .external_force
            .as_mut()
            .ok_or_else(|| not_allocated("external_force"))
    }

    /// Upstream `getInternalForce()`.
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn internal_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.physics
            .internal_force
            .as_ref()
            .ok_or_else(|| not_allocated("internal_force"))?
            .read()
    }

    /// Mutable internal forces.
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn internal_force_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.internal_force_shared_mut()?.write()
    }

    /// The exportable internal forces (zero-copy views for bindings,
    /// D-047).
    ///
    /// # Errors
    /// Before `init_full`.
    pub fn internal_force_shared_mut(&mut self) -> Result<&mut SharedArray<Real>> {
        self.physics
            .internal_force
            .as_mut()
            .ok_or_else(|| not_allocated("internal_force"))
    }

    /// Upstream `getLumpedMass()`: one value per DOF (`nb_nodes ×
    /// nb_degree_of_freedom`), allocated for `_explicit_lumped_mass` or by
    /// the first lumped-mass assembly.
    ///
    /// # Errors
    /// Not allocated (upstream: "The pointer to the mass was not
    /// allocated.").
    pub fn lumped_mass(&self) -> Result<&Array<Real>> {
        self.physics
            .mass
            .as_ref()
            .ok_or_else(|| Error::invalid("The pointer to the mass was not allocated."))
    }

    /// Upstream `getMass()` (alias of [`StructuralMechanicsModel::lumped_mass`]).
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::lumped_mass`].
    pub fn mass(&self) -> Result<&Array<Real>> {
        self.lumped_mass()
    }

    /// Nodal rotary inertia of the lumped mass (not upstream; UD-059,
    /// D-059): one `nr × nr` block per node, row-major (`nr` = 1 rotation in
    /// 2-D, 3 in 3-D: `θx, θy, θz`), allocated with the lumped mass and
    /// written by the lumped-mass assembly. With
    /// [`LumpedRotaryInertia::Block3x3`] it is the full nodal block
    /// `Σ_e Tₑᵀ diag(J_t, J_b, J_b) Tₑ`; with the diagonal rules a diagonal
    /// block. Its diagonal equals the rotational entries of
    /// [`StructuralMechanicsModel::lumped_mass`].
    ///
    /// # Errors
    /// Not allocated (no lumped analysis and no lumped assembly yet).
    pub fn rotary_inertia(&self) -> Result<&Array<Real>> {
        self.physics
            .rotary_inertia
            .as_ref()
            .ok_or_else(|| not_allocated("rotary_inertia"))
    }

    /// The rotary-inertia rule of the lumped mass (UD-059, D-059).
    #[must_use]
    pub fn lumped_rotary_inertia(&self) -> LumpedRotaryInertia {
        self.physics.compat.lumped_rotary_inertia
    }

    /// Upstream `hasLumpedMass()`.
    #[must_use]
    pub fn has_lumped_mass(&self) -> bool {
        self.physics.mass.is_some()
    }

    /// Upstream `allocateLumpedMassArray()` (always `true`).
    ///
    /// # Errors
    /// An unusable mesh domain or an oversized array.
    pub fn allocate_lumped_mass_array(&mut self) -> Result<bool> {
        let n = self.physics.base.mesh.read()?.nb_nodes();
        alloc_nodal_field(&mut self.physics.mass, n, self.physics.nb_degree_of_freedom)?;
        Ok(true)
    }

    // ---- elemental data ------------------------------------------------

    /// Upstream `getStress(type, ghost)`: generalised stresses (`nb_stress`
    /// components per quadrature point: beam 2-D `N, M`; beam 3-D `N, Mz,
    /// My, T`; DKT membrane forces then bending moments), computed by the
    /// internal-force assembly.
    ///
    /// # Errors
    /// Before `init_full`, or no block of that type.
    pub fn stress(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        self.physics.stress.get(ty, ghost)
    }

    /// Every stress block.
    #[must_use]
    pub fn stresses(&self) -> &ElementTypeMapArray<Real> {
        &self.physics.stress
    }

    /// Upstream `getElementMaterial(type, ghost)`: material index of every
    /// element (default `0`).
    ///
    /// # Errors
    /// Before `init_full`, or no block of that type.
    pub fn element_material(
        &self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<ArrayRef<'_, UInt>> {
        self.physics.element_material.get(ty, ghost)
    }

    /// Mutable material indices. Any mutable access marks the stiffness and
    /// both masses for reassembly (UD-508: upstream assembles them once and
    /// ignores later reassignments); writes through other paths are caught
    /// by the content check of the next assembly (see
    /// [`StructuralMechanicsModel::element_material_map_mut`]).
    ///
    /// # Errors
    /// Before `init_full`, or no block of that type.
    pub fn element_material_mut(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<ArrayMut<'_, UInt>> {
        self.physics.invalidate_matrices();
        self.physics.element_material.get_mut(ty, ghost)
    }

    /// The element → material map, mutable, **without** marking anything
    /// (raw access for bindings that export zero-copy views, D-047). Every
    /// assembly compares the map and the materials bitwise with the inputs
    /// of the previous assembly and reassembles when they differ,
    /// so writes through this handle — or through a view built
    /// on it — are never served from stale matrices. Indices are validated
    /// at the next assembly (UD-512).
    pub fn element_material_map_mut(&mut self) -> &mut SharedElementTypeMapArray<UInt> {
        &mut self.physics.element_material
    }

    /// The materials, mutable, **without** marking anything (raw access for
    /// bindings, D-047; see
    /// [`StructuralMechanicsModel::element_material_map_mut`] for the
    /// content check that detects the writes).
    pub fn materials_mut(&mut self) -> &mut [StructuralMaterial] {
        &mut self.physics.materials
    }

    /// Mark the stiffness and both masses for reassembly (explicit hook for
    /// bindings; the content check of every assembly already detects
    /// material and element → material changes, and the FE engine's stamps
    /// refuse edited geometry until [`StructuralMechanicsModel::refresh_geometry`]).
    pub fn invalidate_matrices(&mut self) {
        self.physics.invalidate_matrices();
    }

    /// Upstream `getSet_ID(type, ghost)` ("beam sets": allocated and never
    /// initialised upstream; kept for API parity).
    ///
    /// # Errors
    /// No block of that type (always, unless the user allocated one).
    pub fn set_id(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<UInt>> {
        self.physics.set_id.get(ty, ghost)
    }

    /// The "beam sets" map, mutable.
    pub fn set_id_map_mut(&mut self) -> &mut ElementTypeMapArray<UInt> {
        &mut self.physics.set_id
    }

    /// Rotation matrices of the elements (`ndof × ndof` per element,
    /// column-major; local = `R` · global), precomputed by the FE engine.
    /// Upstream's `getRotationMatrix(type)` reads a model member that is
    /// never filled (UD-513); this is the data it was meant to return.
    ///
    /// # Errors
    /// Before `init_full`, or no block of that type.
    pub fn rotations(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        let mesh = self.physics.base.mesh.read()?;
        self.physics.fe.rotations(&mesh, ty, ghost)
    }

    /// Set the beam normals (`extra_normal` mesh data, `dim` values per
    /// element of `ty`, not ghost) after construction and rebuild the FE
    /// data (the stiffness and the masses are reassembled). A data-only
    /// mesh mutation: the model must be the only consumer of the mesh
    /// (otherwise mutate the mesh with every consumer and call
    /// [`StructuralMechanicsModel::refresh_geometry`]).
    ///
    /// Transactional: for `_bernoulli_beam_3` every new
    /// normal is validated with the element kernel and the model's
    /// compatibility switches **before** the mesh is touched (zero,
    /// parallel and non-finite normals, UD-296; by default also non-unit or
    /// skew normals, UD-343 — see [`StructuralModelUpstreamCompat::fe`]);
    /// if the rebuild still fails, the previous rows are restored and the
    /// FE data rebuilt from them, so a refused normal never stays in the
    /// mesh.
    ///
    /// # Errors
    /// Wrong sizes; other consumers registered; refused normals (mesh and
    /// model unchanged); FE precomputation errors.
    pub fn set_extra_normals(&mut self, ty: ElementType, normals: &[Real]) -> Result<()> {
        let dim = self.physics.base.spatial_dimension;
        let mesh_h = self.physics.base.mesh.clone();
        let previous = {
            let mesh = mesh_h.read()?;
            let n = mesh.nb_element(ty, GhostType::NotGhost);
            if normals.len() != n * dim {
                return Err(Error::DimensionMismatch {
                    context: "set_extra_normals: values (dim per element)",
                    expected: n * dim,
                    got: normals.len(),
                });
            }
            if ty == ElementType::BernoulliBeam3 {
                validate_beam3_normals(&mesh, normals, self.physics.compat.fe)?;
            }
            mesh.data().reals(EXTRA_NORMAL).ok().and_then(|m| {
                m.get(ty, GhostType::NotGhost)
                    .ok()
                    .map(|a| Array::clone(&a))
            })
        };
        let values = Array::from_vec(normals.to_vec(), dim)?;
        self.write_extra_normals(ty, Some(values))?;
        if let Err(err) = self.refresh_geometry() {
            // restore the previous rows (and the FE data built from them)
            self.write_extra_normals(ty, previous)?;
            self.refresh_geometry()?;
            return Err(err);
        }
        Ok(())
    }

    /// Replace (`Some`) or remove (`None`) the `extra_normal` block of `ty`
    /// (not ghost) by a data-only mutation.
    fn write_extra_normals(&mut self, ty: ElementType, values: Option<Array<Real>>) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        mesh_h.mutate(
            &mut [(&self.token, &mut DataOnly as &mut dyn MeshEventHandler)],
            |tx| {
                let map = tx
                    .data_mut(MeshPart::Main)?
                    .reals_mut_or_insert(EXTRA_NORMAL)?;
                match values {
                    Some(v) => {
                        map.as_map_mut().replace(ty, GhostType::NotGhost, v.into());
                    }
                    None => {
                        map.as_map_mut().remove(ty, GhostType::NotGhost);
                    }
                }
                Ok(())
            },
            None,
        )?;
        Ok(())
    }

    /// Rebuild the FE data from the current mesh (nodes, `extra_normal`)
    /// and mark the stiffness and the masses for reassembly (also when the
    /// rebuild fails).
    ///
    /// # Errors
    /// FE precomputation errors: the engine then drops the blocks of the
    /// failing ghost type (D-049), so every later assembly, solve or
    /// accessor reports them missing until a successful refresh.
    pub fn refresh_geometry(&mut self) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let refreshed = self.physics.fe.refresh(&mesh);
        self.physics.invalidate_matrices();
        refreshed
    }

    // ---- materials (STRUCT-002) -----------------------------------------

    /// Upstream `addMaterial(material, name)`: appended, default name
    /// `material_<index>`; returns its index.
    ///
    /// # Errors
    /// A name already used (upstream `AKANTU_EXCEPTION`).
    pub fn add_material(&mut self, material: StructuralMaterial, name: &str) -> Result<usize> {
        let p = &mut self.physics;
        let index = p.materials.len();
        let name = if name.is_empty() {
            format!("material_{index}")
        } else {
            name.to_owned()
        };
        if p.material_names.contains_key(&name) {
            return Err(Error::AlreadyExists {
                kind: "structural material",
                id: format!("{name} (in the model {})", p.base.id),
            });
        }
        if UInt::try_from(index).is_err() {
            return Err(Error::invalid("Can not represent the material ID"));
        }
        p.material_names.insert(name, index);
        p.materials.push(material);
        Ok(index)
    }

    /// Upstream `getMaterial(index)`.
    ///
    /// # Errors
    /// An index out of range.
    pub fn material(&self, index: usize) -> Result<&StructuralMaterial> {
        self.physics.materials.get(index).ok_or(Error::OutOfRange {
            what: "structural material",
            index,
            len: self.physics.materials.len(),
        })
    }

    /// Live handle on a material (UD-023: upstream's Python binding returns
    /// a copy; the C++ accessor is const). Marks the stiffness and both
    /// masses for reassembly (UD-508).
    ///
    /// # Errors
    /// An index out of range.
    pub fn material_mut(&mut self, index: usize) -> Result<&mut StructuralMaterial> {
        let len = self.physics.materials.len();
        self.physics.invalidate_matrices();
        self.physics
            .materials
            .get_mut(index)
            .ok_or(Error::OutOfRange {
                what: "structural material",
                index,
                len,
            })
    }

    /// Upstream `getMaterial(name)`.
    ///
    /// # Errors
    /// No material of that name.
    pub fn material_by_name(&self, name: &str) -> Result<&StructuralMaterial> {
        let i = self.material_index(name)?;
        self.material(i)
    }

    /// Index of the material `name`.
    ///
    /// # Errors
    /// No material of that name.
    pub fn material_index(&self, name: &str) -> Result<usize> {
        self.physics
            .material_names
            .get(name)
            .copied()
            .ok_or_else(|| {
                Error::not_found(
                    "structural material",
                    format!("{name} (in the model {})", self.physics.base.id),
                )
            })
    }

    /// Upstream `getMaterialByElement(element)`.
    ///
    /// # Errors
    /// Unknown element, or a material index out of range.
    pub fn material_by_element(&self, element: Element) -> Result<&StructuralMaterial> {
        let i = *self
            .physics
            .element_material
            .element_ref(element)?
            .first()
            .ok_or_else(|| Error::invalid("empty material map tuple"))?;
        self.material(i as usize)
    }

    /// Upstream `getNbMaterials()`.
    #[must_use]
    pub fn nb_materials(&self) -> usize {
        self.physics.materials.len()
    }

    // ---- explicit assembly entry points -------------------------------

    /// Upstream `assembleStiffnessMatrix()` (once, unless an input changed:
    /// UD-508). One collective operation of the DOF manager.
    ///
    /// # Errors
    /// Assembly errors (bad material indices, stale FE data, …).
    pub fn assemble_stiffness_matrix(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("structural:assemble_stiffness", |dm| {
                p.assemble_stiffness_matrix(dm)?;
                dm.finalize_matrix("K")
            })
    }

    /// Upstream `assembleMassMatrix()` (consistent mass `M`, once unless an
    /// input changed).
    ///
    /// # Errors
    /// Assembly errors (DKT has no shape functions upstream, UD-053).
    pub fn assemble_mass_matrix(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("structural:assemble_mass", |dm| {
                p.assemble_mass_matrix(dm)?;
                dm.finalize_matrix("M")
            })
    }

    /// Upstream `assembleLumpedMassMatrix()` (beams only; UD-005/UD-059).
    ///
    /// # Errors
    /// Non-beam elements (upstream: "The lumped mass was not implemented
    /// for non Bernoulli Beams"); bad material indices.
    pub fn assemble_lumped_mass_matrix(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("structural:assemble_lumped_mass", |dm| {
                p.assemble_lumped_mass_matrix(dm)
            })
    }

    /// Upstream `assembleMatrix(id)` (`K`, `M`; other ids ignored).
    ///
    /// # Errors
    /// Assembly errors.
    pub fn assemble_matrix(&mut self, matrix_id: &str) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("structural:assemble_matrix", |dm| {
                SolverCallback::assemble_matrix(p, dm, matrix_id)?;
                if dm.has_matrix(matrix_id) {
                    dm.finalize_matrix(matrix_id)?;
                }
                Ok(())
            })
    }

    /// Upstream `assembleLumpedMatrix(id)` (`M`; other ids ignored).
    ///
    /// # Errors
    /// Assembly errors.
    pub fn assemble_lumped_matrix(&mut self, matrix_id: &str) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("structural:assemble_lumped_matrix", |dm| {
                SolverCallback::assemble_lumped_matrix(p, dm, matrix_id)
            })
    }

    /// Upstream `computeStresses()`: `σ = D·B·u_e` at every quadrature point
    /// of the structural types of the model dimension (not ghost).
    ///
    /// # Errors
    /// Bad material indices, stale FE data, malformed displacement.
    pub fn compute_stresses(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("structural:compute_stresses", |dm| p.compute_stresses(dm))
    }

    /// Upstream `assembleInternalForce()`: zero, stresses, `f_int −= ∫ Bᵀσ`.
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::compute_stresses`].
    pub fn assemble_internal_force(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("structural:assemble_internal_force", |dm| {
                p.assemble_internal_force(dm)
            })
    }

    /// Upstream `assembleResidual()`: **accumulates** `f_ext + f_int` into
    /// the DOF manager's residual (the solvers zero it before calling the
    /// callback; zero it first with `dof_manager_mut().zero_residual()`).
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::assemble_internal_force`].
    pub fn assemble_residual(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("structural:assemble_residual", |dm| p.assemble_residual(dm))
    }
}

impl StructuralMechanicsPhysics {
    /// Upstream `getDefaultSolverID(method)` (STRUCT-008): `_static` →
    /// `static`, `_implicit_dynamic` → `implicit`, `_explicit_lumped_mass`
    /// → `explicit_lumped`, `_explicit_consistent_mass` → `explicit`;
    /// otherwise upstream prints `"UNKOWN."` and returns `("unknown",
    /// _not_defined)` (the port prints nothing, UD-509).
    #[must_use]
    pub fn default_solver_id(&self, method: AnalysisMethod) -> (Id, TimeStepSolverType) {
        match method {
            AnalysisMethod::Static => ("static".into(), TimeStepSolverType::Static),
            AnalysisMethod::ImplicitDynamic => ("implicit".into(), TimeStepSolverType::Dynamic),
            AnalysisMethod::ExplicitLumpedMass => {
                ("explicit_lumped".into(), TimeStepSolverType::DynamicLumped)
            }
            AnalysisMethod::ExplicitConsistentMass => {
                ("explicit".into(), TimeStepSolverType::Dynamic)
            }
            _ => ("unknown".into(), TimeStepSolverType::NotDefined),
        }
    }

    /// The structural FE engine.
    #[must_use]
    pub fn fe_engine(&self) -> &StructuralFeEngine {
        &self.fe
    }

    /// Materials, in insertion order.
    #[must_use]
    pub fn materials(&self) -> &[StructuralMaterial] {
        &self.materials
    }

    /// Mark the stiffness and both masses for reassembly (UD-508).
    pub(crate) fn invalidate_matrices(&mut self) {
        self.assembled_stiffness = None;
        self.assembled_mass = None;
        self.assembled_lumped_mass = None;
    }

    /// `initFullImpl`'s stress initialisation: every structural type (all
    /// dimensions, both ghost types), `nb_element · nb_quad` tuples of
    /// `nb_stress` components, existing values kept.
    fn init_stresses(&mut self) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        for g in GhostType::ALL {
            for ty in structural_types(&mesh, g) {
                let ns = nb_stress_components(ty)?;
                let nq = self.fe.nb_integration_points(ty, g)?;
                let n = mesh
                    .nb_element(ty, g)
                    .checked_mul(nq)
                    .ok_or_else(|| Error::invalid("stress size overflow"))?;
                self.stress.alloc(n, ns, ty, g, 0.0)?;
            }
        }
        Ok(())
    }
}

impl SolverCallback for StructuralMechanicsPhysics {
    /// Upstream `getMatrixType(id)`: `_symmetric` for every id (`C`
    /// included: the second-order schemes create an empty `C`).
    fn matrix_type(&self, _matrix_id: &str) -> MatrixType {
        MatrixType::Symmetric
    }

    /// Upstream `assembleMatrix(id)`: `K` → stiffness, `M` → consistent
    /// mass, anything else ignored.
    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        match matrix_id {
            "K" => self.assemble_stiffness_matrix(dofs),
            "M" => self.assemble_mass_matrix(dofs),
            _ => Ok(()),
        }
    }

    /// Upstream `assembleLumpedMatrix(id)`: `M` → lumped mass.
    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if matrix_id == "M" {
            self.assemble_lumped_mass_matrix(dofs)
        } else {
            Ok(())
        }
    }

    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        StructuralMechanicsPhysics::assemble_residual(self, dofs)
    }

    /// Upstream `assembleResidual(part)`: `external`, `internal`, otherwise
    /// `SolverCallbackResidualPartUnknown`.
    fn assemble_residual_part(&mut self, dofs: &mut DofManager, part: &str) -> Result<()> {
        match part {
            "external" => {
                let f = self
                    .external_force
                    .as_ref()
                    .ok_or_else(|| not_allocated("external_force"))?
                    .read()?;
                dofs.assemble_to_residual(DOF_ID, f.view(), 1.0)
            }
            "internal" => {
                self.assemble_internal_force(dofs)?;
                let f = self
                    .internal_force
                    .as_ref()
                    .ok_or_else(|| not_allocated("internal_force"))?
                    .read()?;
                dofs.assemble_to_residual(DOF_ID, f.view(), 1.0)
            }
            other => Err(Error::invalid(format!(
                "The residual part {other} is unknown by the solver callback"
            ))),
        }
    }

    fn can_split_residual(&self) -> bool {
        true
    }

    /// Upstream `afterSolveStep(converged)`: the internal forces of a
    /// converged step.
    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        if converged {
            self.assemble_internal_force(dofs)?;
        }
        Ok(())
    }

    /// Upstream `initSolver(type)` (STRUCT-001): `external_force`,
    /// `internal_force` (`nb_degree_of_freedom` components); the family
    /// `displacement` and its blocked flags if missing; velocity and
    /// acceleration for dynamic solvers; the lumped mass array for
    /// `dynamic_lumped`.
    fn init_solver(&mut self, dofs: &mut DofManager, ty: TimeStepSolverType) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let n = mesh.nb_nodes();
        let ndof = self.nb_degree_of_freedom;
        drop(alloc_nodal_shared_field(&mut self.external_force, n, ndof)?);
        drop(alloc_nodal_shared_field(&mut self.internal_force, n, ndof)?);
        if !dofs.has_dofs(DOF_ID) {
            dofs.register_nodal_dofs(&mesh, DOF_ID, ndof)?;
            dofs.register_blocked_dofs(DOF_ID)?;
        }
        if matches!(
            ty,
            TimeStepSolverType::Dynamic | TimeStepSolverType::DynamicLumped
        ) {
            if !dofs.has_dofs_derivative(DOF_ID, 1)? {
                dofs.register_dofs_derivative(DOF_ID, 1)?;
                dofs.register_dofs_derivative(DOF_ID, 2)?;
            }
            if ty == TimeStepSolverType::DynamicLumped {
                alloc_nodal_field(&mut self.mass, n, ndof)?;
                let nr = ndof - self.base.spatial_dimension;
                alloc_nodal_field(&mut self.rotary_inertia, n, nr * nr)?;
            }
        }
        Ok(())
    }

    /// Upstream `getDefaultSolverOptions(type)` (STRUCT-008).
    fn default_solver_options(&self, ty: TimeStepSolverType) -> ModelSolverOptions {
        let mut o = ModelSolverOptions::default();
        let (nls, scheme, st) = match ty {
            TimeStepSolverType::DynamicLumped => (
                NonLinearSolverType::Lumped,
                IntegrationSchemeType::CentralDifference,
                SolutionType::Acceleration,
            ),
            TimeStepSolverType::Static => (
                NonLinearSolverType::NewtonRaphson,
                IntegrationSchemeType::PseudoTime,
                SolutionType::NotDefined,
            ),
            TimeStepSolverType::Dynamic => {
                if self.base.method == AnalysisMethod::ExplicitConsistentMass {
                    (
                        NonLinearSolverType::NewtonRaphson,
                        IntegrationSchemeType::CentralDifference,
                        SolutionType::Acceleration,
                    )
                } else {
                    (
                        NonLinearSolverType::NewtonRaphson,
                        IntegrationSchemeType::TrapezoidalRule2,
                        SolutionType::Displacement,
                    )
                }
            }
            // upstream: AKANTU_EXCEPTION(type << " is not a valid time step
            // solver type"); the options stay unset and getNewSolver fails
            TimeStepSolverType::NotDefined => return o,
        };
        o.non_linear_solver_type = Some(nls);
        o.integration_scheme_type.insert(DOF_ID.into(), scheme);
        o.solution_type.insert(DOF_ID.into(), st);
        o
    }
}

impl MeshEventHandler for StructuralMechanicsModel {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }

    fn event_priorities(&self) -> Vec<EventPriority> {
        vec![
            EventPriority::FE_ENGINE,
            EventPriority::DOF_MANAGER,
            EventPriority::MODEL,
        ]
    }

    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.on_mesh_event_at(mesh, event, EventPriority::MODEL)
    }

    /// The refusals of the dispatch, before the commit:
    /// every topology event (`Error::NotImplemented`, UD-342) and
    /// `MeshIsDistributed` (`Error::Unsupported`, UD-131) are refused here,
    /// so `SharedMesh::mutate` rolls the mutation back on every rank and the
    /// mesh domain stays usable. Same rule as the dispatch
    /// (`structural_event_refusal`). Nothing to forward: the model's FE
    /// engine and DOF manager are owned components, not registered
    /// consumers, and the dispatch never forwards an event to them (every
    /// event they would have to handle is refused).
    fn check_mesh_event(&self, _mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        structural_event_refusal(event)
    }

    /// Upstream's structural model does not support mesh changes: its FE
    /// engine throws on element events (`ShapeStructural` does not override
    /// `onElementsAdded/Removed`, UD-342) and its nodal arrays, material
    /// map and stresses are never resized. Every topology event is refused
    /// at the first level (`structural_event_refusal`). Through
    /// `SharedMesh::mutate` the capability check
    /// ([`MeshEventHandler::check_mesh_event`]) refuses the same events
    /// before the commit, so this dispatch refusal is reached only when the
    /// handler is called directly (a refusal during a dispatch would poison
    /// the mesh domain).
    /// A future event-capable version must extend `extra_normal` in the same
    /// `SharedMesh` transaction as every new `_bernoulli_beam_3` element
    /// (D-049), resize the model arrays and refresh the engine.
    fn on_mesh_event_at(
        &mut self,
        _mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
    ) -> Result<()> {
        if level != EventPriority::FE_ENGINE {
            return Ok(());
        }
        structural_event_refusal(event)
    }
}

/// The mesh events the structural model refuses, shared by its capability
/// check and its dispatch: topology events →
/// `Error::NotImplemented` (UD-342), `MeshIsDistributed` →
/// `Error::Unsupported` (UD-131), every other event `Ok(())`.
fn structural_event_refusal(event: &MeshEvent) -> Result<()> {
    match event {
        MeshEvent::NodesAdded { .. }
        | MeshEvent::NodesRemoved { .. }
        | MeshEvent::ElementsAdded { .. }
        | MeshEvent::ElementsRemoved { .. }
        | MeshEvent::ElementsChanged { .. } => Err(Error::NotImplemented(
            "StructuralMechanicsModel: mesh topology changes (upstream's structural FE \
             engine and model do not handle mesh events, UD-342)",
        )),
        // Unreachable through `SharedMesh` today
        // (distribution needs a consumer-less mesh), refused explicitly
        MeshEvent::MeshIsDistributed { .. } => Err(Error::Unsupported(
            "StructuralMechanicsModel is serial: the mesh cannot be distributed while the \
             model uses it (upstream has no synchronizer; UD-131)"
                .into(),
        )),
        // non-topological events (and future variants: #[non_exhaustive])
        _ => Ok(()),
    }
}

impl Model for StructuralMechanicsModel {
    fn model_type(&self) -> ModelType {
        ModelType::StructuralMechanicsModel
    }

    fn mesh(&self) -> &SharedMesh {
        &self.physics.base.mesh
    }

    fn mesh_token(&self) -> &ConsumerToken {
        &self.token
    }

    fn model_solver(&self) -> &ModelSolver {
        &self.solver
    }

    fn model_solver_mut(&mut self) -> &mut ModelSolver {
        &mut self.solver
    }

    fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        StructuralMechanicsModel::init_full(self, options)
    }

    fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        StructuralMechanicsModel::solve_step(self, solver_id)
    }

    /// Upstream's structural model has no stable time step (the `Model`
    /// virtual is not overridden: `AKANTU_TO_IMPLEMENT`).
    fn stable_time_step(&mut self) -> Result<Real> {
        Err(Error::NotImplemented(
            "StructuralMechanicsModel::getStableTimeStep (not implemented upstream)",
        ))
    }

    fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        StructuralMechanicsModel::dump(self, time, step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_mesh::MeshAccessor;

    /// The data-only handler refuses every event in the
    /// capability check, before the commit (as its dispatch does): a
    /// "data-only" mutation that records an event is rolled back
    /// recoverably, and the mesh domain stays usable.
    #[test]
    fn fu2_01_data_only_refuses_events_before_commit() {
        let mut mesh = Mesh::new(2, "data_only").unwrap();
        {
            let mut a = MeshAccessor::new(&mut mesh);
            a.push_node(&[0., 0.]).unwrap();
            a.push_node(&[1., 0.]).unwrap();
        }
        let shared = SharedMesh::new(mesh);
        let token = shared
            .register_multilevel_consumer(
                "data_only",
                &[
                    EventPriority::FE_ENGINE,
                    EventPriority::DOF_MANAGER,
                    EventPriority::MODEL,
                ],
                MeshPart::Main,
            )
            .unwrap();
        let distributed = MeshEvent::MeshIsDistributed {
            origin: "fu2_01".into(),
        };
        let direct = DataOnly.check_mesh_event(&shared.read().unwrap(), &distributed);
        assert!(
            matches!(&direct, Err(Error::InvalidArgument(t)) if t.contains("data-only")),
            "{direct:?}"
        );
        let revision = shared.revision().unwrap();
        let r = shared.mutate(
            &mut [(&token, &mut DataOnly as &mut dyn MeshEventHandler)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[2., 0.])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "fu2_01".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        );
        let Err(e) = &r else {
            panic!("expected the data-only refusal, got {r:?}");
        };
        assert!(
            matches!(e, Error::InvalidArgument(t) if t.contains("data-only")),
            "{e:?}"
        );
        assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
        assert_eq!(shared.read().unwrap().nb_nodes(), 2);
        assert_eq!(shared.revision().unwrap(), revision);
        let again = shared.mutate(
            &mut [(&token, &mut DataOnly as &mut dyn MeshEventHandler)],
            |_| Ok(()),
            None,
        );
        assert!(again.is_ok(), "{again:?}");
    }
}
