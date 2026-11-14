// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/contact_mechanics/contact_mechanics_model.hh
// (member arrays), contact_mechanics_model.cc (initSolver, savePreviousState).

//! Nodal fields of the contact model and the upstream-compatibility
//! switches.

use akantu_core::enums::ContactState;
use akantu_core::{Array, Element, Real, Result, SharedArray};

/// Nodal arrays of the contact model (upstream members allocated by
/// `initSolver`, all with one row per node of the detection mesh).
///
/// The arrays upstream's Python module returns by reference
/// (`getExternalForce`, `getNormalForce`, `getTangentialForce`,
/// `getInternalForce`, `getGaps`, `getNormals`, `getNodalArea`,
/// `py_contact_mechanics_model.cc:150-156`) are exportable storage
/// (`SharedArray`, D-047, D-078): zero-copy views for the
/// bindings, reached natively through guards. The per-search fields are
/// resized to the same length and refilled **in place** by every search
/// (views stay attached, as upstream's references); a resize to another
/// length (cohesive insertion) retires the block (views stale but safe,
/// UD-085).
#[derive(Clone, Debug)]
pub struct ContactFields {
    /// `displacement` (`dim`; set by users / boundary conditions only).
    pub displacement: Array<Real>,
    /// `displacement_increment` (`dim`).
    pub displacement_increment: Array<Real>,
    /// `internal_force` = contact force `f_N + f_T` (`dim`).
    pub internal_force: SharedArray<Real>,
    /// `external_force` (`dim`; scratch of the nodal-area computation,
    /// emptied after it as upstream's `clear()`).
    pub external_force: SharedArray<Real>,
    /// `normal_force` (`dim`).
    pub normal_force: SharedArray<Real>,
    /// `tangential_force` (`dim`).
    pub tangential_force: SharedArray<Real>,
    /// `gaps` (1; positive = penetration after `search`).
    pub gaps: SharedArray<Real>,
    /// `areas` (1; norm of `∫ Nᵀ n` over the contact surface).
    pub nodal_area: SharedArray<Real>,
    /// `blocked_dofs` (1 component, as upstream).
    pub blocked_dofs: Array<bool>,
    /// `contact_state` (1).
    pub contact_state: Array<ContactState>,
    /// `previous_master_elements` (1).
    pub previous_master_elements: Array<Element>,
    /// `normals` (`dim`).
    pub normals: SharedArray<Real>,
    /// `tangents` (`dim·(dim − 1)`, column-major `dim × (dim − 1)` per node).
    pub tangents: Array<Real>,
    /// `previous_tangents`.
    pub previous_tangents: Array<Real>,
    /// `projections` (`dim − 1`).
    pub projections: Array<Real>,
    /// `previous_projections`.
    pub previous_projections: Array<Real>,
    /// `tangential_tractions` (`dim − 1`).
    pub tangential_tractions: Array<Real>,
    /// `previous_tangential_tractions`.
    pub previous_tangential_tractions: Array<Real>,
    allocated: bool,
}

impl ContactFields {
    /// Unallocated fields of a model of dimension `dim` (empty arrays with
    /// the right number of components).
    #[must_use]
    pub fn new(dim: usize) -> Self {
        let sd = dim.saturating_sub(1);
        Self {
            displacement: Array::with_id(dim, "displacement"),
            displacement_increment: Array::with_id(dim, "displacement_increment"),
            internal_force: SharedArray::new(Array::with_id(dim, "internal_force")),
            external_force: SharedArray::new(Array::with_id(dim, "external_force")),
            normal_force: SharedArray::new(Array::with_id(dim, "normal_force")),
            tangential_force: SharedArray::new(Array::with_id(dim, "tangential_force")),
            gaps: SharedArray::new(Array::with_id(1, "gaps")),
            nodal_area: SharedArray::new(Array::with_id(1, "areas")),
            blocked_dofs: Array::with_id(1, "blocked_dofs"),
            contact_state: Array::with_id(1, "contact_state"),
            previous_master_elements: Array::with_id(1, "previous_master_elements"),
            normals: SharedArray::new(Array::with_id(dim, "normals")),
            tangents: Array::with_id(sd * dim, "tangents"),
            previous_tangents: Array::with_id(sd * dim, "previous_tangents"),
            projections: Array::with_id(sd, "projections"),
            previous_projections: Array::with_id(sd, "previous_projections"),
            tangential_tractions: Array::with_id(sd, "tangential_tractions"),
            previous_tangential_tractions: Array::with_id(sd, "previous_tangential_tractions"),
            allocated: false,
        }
    }

    /// `true` once [`ContactFields::allocate`] ran.
    #[must_use]
    pub fn is_allocated(&self) -> bool {
        self.allocated
    }

    /// Upstream `initSolver`: allocate every field with `nb_nodes` rows
    /// (zeros / `_no_contact` / `ElementNull`), once (upstream
    /// `allocNodalField` keeps existing arrays).
    pub fn allocate(&mut self, nb_nodes: usize) -> Result<()> {
        if self.allocated {
            return Ok(());
        }
        for a in self.shared_mut() {
            a.write()?.try_resize(nb_nodes, 0.0)?;
        }
        for a in [
            &mut self.displacement,
            &mut self.displacement_increment,
            &mut self.tangents,
            &mut self.previous_tangents,
            &mut self.projections,
            &mut self.previous_projections,
            &mut self.tangential_tractions,
            &mut self.previous_tangential_tractions,
        ] {
            a.try_resize(nb_nodes, 0.0)?;
        }
        self.blocked_dofs.try_resize(nb_nodes, false)?;
        self.contact_state
            .try_resize(nb_nodes, ContactState::NoContact)?;
        self.previous_master_elements
            .try_resize(nb_nodes, Element::NULL)?;
        self.allocated = true;
        Ok(())
    }

    /// The exportable fields (upstream's by-reference getters).
    pub fn shared_mut(&mut self) -> [&mut SharedArray<Real>; 7] {
        [
            &mut self.internal_force,
            &mut self.external_force,
            &mut self.normal_force,
            &mut self.tangential_force,
            &mut self.gaps,
            &mut self.nodal_area,
            &mut self.normals,
        ]
    }
}

/// How the contact force of a deformable master is distributed (UD-002;
/// a two-pass formulation may require weights, so the weight is
/// a formulation choice applied consistently to force **and** stiffness).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeformableMasterWeighting {
    /// Two-pass weighting (default; upstream's force weight): every node of
    /// a contact element with a deformable master receives `½ ·` its local
    /// force, and the stiffness is weighted by `½` too. Intended for
    /// symmetric selections (each surface both master and slave).
    #[default]
    TwoPassHalf,
    /// One-pass: full force and stiffness on every node.
    OnePassFull,
}

impl DeformableMasterWeighting {
    /// Weight of a deformable master's contact element.
    #[must_use]
    pub const fn weight(self) -> Real {
        match self {
            Self::TwoPassHalf => 0.5,
            Self::OnePassFull => 1.0,
        }
    }
}

/// Upstream-compatibility switches (D-003 category B defects). Default
/// (`false` everywhere): corrected behaviour. Differential replays against
/// the unpatched oracle set the switches of the defects a case reaches.
/// Category A defects (UD-001, UD-362) have no switch (undefined behaviour
/// upstream).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContactUpstreamCompat {
    /// **UD-002**: nodal contact forces are **assigned** (`=`; the last
    /// contact element touching a node wins) with weight ½ for deformable
    /// masters; the stiffness is added unweighted with `add(i, j)` at
    /// `node·dim + d` (mesh node numbers, not DOF-manager equations), which
    /// doubles symmetric off-diagonal terms in symmetric storage.
    pub assembly: bool,
    /// **UD-363**: the natural-projection Newton uses the top-left block of
    /// `Gᵀ G` instead of the surface metric `G Gᵀ`.
    pub projection_metric: bool,
    /// **UD-364**: the derivative shape matrices `A_β` read `∂N/∂ξ` as
    /// consecutive chunks of its column-major storage (scrambled in 3-D;
    /// identical in 2-D).
    pub derivative_shape_chunks: bool,
    /// **UD-368**: the penalty normal moduli use upstream's
    /// Konyukhov–Schweizerhof expression (unit tangents, rotational terms
    /// of the wrong sign, no `1/|∂x/∂ξ|` scaling, no `g²` and curvature
    /// terms), which is not the derivative of the penalty normal force.
    /// Default: the consistent tangent of the force with the nodal area
    /// frozen.
    pub normal_moduli: bool,
    /// **UD-008**: the second rotational term of the quadratic penalty's
    /// normal stiffness uses `n ⊗ τ` instead of `τ ⊗ n` (only meaningful
    /// with [`ContactUpstreamCompat::normal_moduli`]: the consistent
    /// tangent has no such term).
    pub quadratic_rotational_dyad: bool,
    /// **UD-365**: the trial tangential traction evaluates the **current**
    /// natural projection on the **previous** master element's nodes (with
    /// the current element's type), so a slave crossing to another master
    /// facet sees a spurious slip increment.
    pub trial_slip_on_previous_element: bool,
    /// **UD-369**: the penalty friction moduli are upstream's stick/slip
    /// expressions (UD-362 `θ` loop over the tangents, UD-001 `k_third`
    /// zero-initialised), which are not the derivative of the frictional
    /// force (every term has the opposite sign; in 3-D slip the return-map
    /// term is divided by the returned norm `μ p_N` instead of the trial
    /// norm). Default: the consistent tangent `−∂f_T/∂u` with the history
    /// and the nodal area frozen.
    pub friction_moduli: bool,
    /// **UD-371**: facet normals are oriented outward with the barycentres
    /// of the **reference** mesh nodes (upstream `outsideDirection`) in the
    /// projection and the self-contact filter, so a rigid rotation of more
    /// than 90° loses every contact. Default: the same barycentres on the
    /// current positions.
    pub normal_orientation: bool,
    /// **UD-372**: a projection on a triangle facet is accepted when its
    /// natural coordinates lie in the box `[−1 − tol, 1 + tol]²` (points
    /// outside the triangle included), and a coplanar or flatter neighbour
    /// whose projection lies outside it can win. Default: a facet whose
    /// projection lies in it (barycentric coordinates `≥ −tol`) is
    /// preferred; the box decides only when no facet around the master node
    /// contains the projection (no contact upstream finds is lost).
    pub projection_validity: bool,
}

impl ContactUpstreamCompat {
    /// Every switch on (reproduces upstream wherever it is defined).
    #[must_use]
    pub const fn all() -> Self {
        Self {
            assembly: true,
            projection_metric: true,
            derivative_shape_chunks: true,
            normal_moduli: true,
            quadratic_rotational_dyad: true,
            trial_slip_on_previous_element: true,
            friction_moduli: true,
            normal_orientation: true,
            projection_validity: true,
        }
    }
}
