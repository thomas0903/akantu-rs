// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/contact_mechanics/resolution.hh,
// resolution.cc, resolution_utils.hh, resolution_utils.cc,
// resolutions/resolution_penalty.hh, resolutions/resolution_penalty.cc,
// resolutions/resolution_penalty_quadratic.hh,
// resolutions/resolution_penalty_quadratic.cc.

//! Contact resolutions (CONT-014…016): the `Resolution` interface, the
//! factory, the shape-function matrices of a contact element and the
//! penalty resolutions (`penalty_linear`, `penalty_quadratic`) with
//! Coulomb friction.
//!
//! Formulas (Konyukhov–Schweizerhof, upstream comments): with `A = [I,
//! −N₁I, …, −N_nI]` (`dim × dim(n+1)`), `A_β = [0, ∂_βN₁I, …]`, unit
//! covariant tangents `τ_α`, metric `a_αβ = τ_α·τ_β`, `a^{αβ} = (a)⁻¹`:
//! * normal traction `p_N = ε_n⟨g⟩` (quadratic: `ε_n(⟨g⟩² + ⟨g⟩)`);
//!   force `f_N = p_N A_s Aᵀ n`;
//! * friction (μ ≠ 0, slave in contact at the previous search): trial
//!   traction transported from the previous tangents minus `ε_t a Δξ`;
//!   stick if `‖t‖ ≤ μ p_N` (`‖t‖² = t a⁻¹ t`), else slip
//!   `t = μ p_N t/‖t‖`; force `f_T = A_s Σ t_β a^{αβ} Aᵀ τ_α`;
//! * moduli: consistent tangents `−∂f/∂u` by default (UD-368, UD-369),
//!   upstream's normal, stick and slip moduli in compatibility mode.
//!
//! Every Eigen expression keeps upstream's association and evaluation
//! order (`crate::eigen_ops`); workspaces are allocated once per assembly
//! ([`Scratch`]), never per contact element.
//!
//! Deviations: **UD-001** (zero-initialised `traction_trial`,
//! `increment_xi`, `k_third`), **UD-362** (stick moduli: `θ` runs over the
//! `dim − 1` tangents; upstream iterates the raw storage of `τᵀ`,
//! mismatching product sizes and reading the metric out of bounds),
//! **UD-364**, **UD-008**, **UD-365**, **UD-368**, **UD-369** (switches in
//! [`ContactUpstreamCompat`]): by default the normal and friction moduli are
//! the consistent tangents of the forces with the nodal area frozen (the
//! friction tangent also with the history frozen), upstream's expressions
//! are compatibility modes.

use std::collections::BTreeMap;
use std::fmt;

use akantu_core::eigen_order::sum_from_zero;
use akantu_core::enums::ContactState;
use akantu_core::{Array, Error, Id, Idx, Real, Result, try_to_usize};
use akantu_mesh::Mesh;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    table_get, table_info, table_names, table_set, table_set_from_input,
};
use akantu_parser::{ByteString, Parsable, ParserType};

use crate::eigen_ops::{
    SMALL_MAX, dot, gemv_scaled_transposed, gram, inverse_gram, matmul, norm, outer,
};
use crate::element::ContactElement;
use crate::fields::{ContactFields, ContactUpstreamCompat};
use crate::geometry::{MAX_FACET_NODES, dnds_at, real_projection, shapes_at};

/// Largest number of DOFs of a contact element (`3 · (8 + 1)`).
pub const MAX_ELEMENT_DOFS: usize = 3 * (MAX_FACET_NODES + 1);

/// Upstream `macaulay(x) = std::max(x, 0)`.
#[inline]
#[must_use]
pub fn macaulay(x: Real) -> Real {
    if x < 0.0 { 0.0 } else { x }
}

/// Upstream `heaviside(x) = x < 0 ? 0 : 1` (`heaviside(0) = 1`).
#[inline]
#[must_use]
pub fn heaviside(x: Real) -> Real {
    if x < 0.0 { 0.0 } else { 1.0 }
}

/// State a resolution reads (and, for friction, writes) during an assembly.
#[derive(Debug)]
pub struct ResolutionContext<'a> {
    /// Detection mesh (master facets' connectivities).
    pub mesh: &'a Mesh,
    /// Current positions (the detector's).
    pub positions: &'a Array<Real>,
    /// Spatial dimension.
    pub dim: usize,
    /// The model's nodal fields.
    pub fields: &'a mut ContactFields,
    /// Upstream-compatibility switches.
    pub compat: ContactUpstreamCompat,
}

impl ResolutionContext<'_> {
    fn sd(&self) -> usize {
        self.dim - 1
    }
}

/// Workspaces of the moduli computations (sized for the largest contact
/// element; allocated once per assembly).
#[derive(Clone, Debug)]
pub struct Scratch {
    a: Vec<Real>,
    ajs: Vec<Real>,
    t1: Vec<Real>,
    p: Vec<Real>,
    p2: Vec<Real>,
    m1: Vec<Real>,
    m2: Vec<Real>,
    m3: Vec<Real>,
    m4: Vec<Real>,
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}

impl Scratch {
    /// Workspaces for contact elements of at most [`MAX_ELEMENT_DOFS`] DOFs.
    #[must_use]
    pub fn new() -> Self {
        let nd = MAX_ELEMENT_DOFS;
        Self {
            a: vec![0.0; 3 * nd],
            ajs: vec![0.0; 2 * 3 * nd],
            t1: vec![0.0; 3 * nd],
            p: vec![0.0; nd * nd],
            p2: vec![0.0; nd * nd],
            m1: vec![0.0; nd * nd],
            m2: vec![0.0; nd * nd],
            m3: vec![0.0; nd * nd],
            m4: vec![0.0; nd * nd],
        }
    }
}

/// Upstream `ResolutionUtils::computeShapeFunctionMatrix`: `A` (`dim ×
/// dim(n+1)`, column-major) at the natural projection `xi`. Returns the
/// number of columns `nd`.
pub fn shape_function_matrix(
    master: akantu_core::ElementType,
    dim: usize,
    xi: &[Real],
    a: &mut [Real],
) -> Result<usize> {
    let nn = master.nb_nodes_per_element();
    let nd = dim * (nn + 1);
    let mut n = [0.0; MAX_FACET_NODES];
    shapes_at(master, xi, &mut n[..nn])?;
    let a = a
        .get_mut(..dim * nd)
        .ok_or_else(|| Error::invalid("shape_function_matrix: workspace too small"))?;
    a.fill(0.0);
    for i in 0..=nn {
        for j in 0..dim {
            a[j + (i * dim + j) * dim] = if i == 0 { 1.0 } else { -n[i - 1] };
        }
    }
    Ok(nd)
}

/// Upstream `ResolutionUtils::computeDerivativeShapeFunctionMatrix`: the
/// `dim − 1` matrices `A_β = [0, ∂_βN₁ I, …]` (each `dim × nd`, stored one
/// after the other). With `raw_chunks` (**UD-364** compatibility) the
/// `β`-th vector of derivatives is the `β`-th chunk of `nn` consecutive
/// values of the column-major `∂N/∂ξ` storage, as upstream's iteration over
/// `shape_derivatives.transpose()` reads it (identical for `dim − 1 = 1`).
pub fn derivative_shape_function_matrices(
    master: akantu_core::ElementType,
    dim: usize,
    xi: &[Real],
    raw_chunks: bool,
    out: &mut [Real],
) -> Result<()> {
    let sd = dim - 1;
    let nn = master.nb_nodes_per_element();
    let nd = dim * (nn + 1);
    let mut d = [0.0; 2 * MAX_FACET_NODES];
    dnds_at(master, xi, &mut d[..sd * nn])?;
    let out = out
        .get_mut(..sd * dim * nd)
        .ok_or_else(|| Error::invalid("derivative_shape_function_matrices: workspace too small"))?;
    out.fill(0.0);
    for beta in 0..sd {
        let aj = &mut out[beta * dim * nd..(beta + 1) * dim * nd];
        for i in 1..=nn {
            let k = i - 1;
            let v = if raw_chunks {
                d[beta * nn + k]
            } else {
                d[beta + k * sd]
            };
            for j in 0..dim {
                aj[j + (i * dim + j) * dim] = v;
            }
        }
    }
    Ok(())
}

/// Interface of a contact resolution (upstream `Resolution`, a `Parsable`
/// with parameters `name`, `mu`, `is_master_deformable` plus the
/// resolution's own).
pub trait Resolution: ParameterRegistry + fmt::Debug + Send {
    /// Identifier (`<model id>:<index>:<type>`).
    fn id(&self) -> &str;
    /// Resolution type (`penalty_linear`, …).
    fn resolution_type(&self) -> &str;
    /// Friction coefficient.
    fn mu(&self) -> Real;
    /// Whether master facets are deformable (forces/stiffness on every node
    /// of the contact element instead of the slave only).
    fn is_master_deformable(&self) -> bool;
    /// Local normal force (`nd` values) of `element` (upstream
    /// `computeNormalForce`).
    fn compute_normal_force(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        force: &mut [Real],
    ) -> Result<()>;
    /// Local tangential force (upstream `computeTangentialForce`); updates
    /// the slave's tangential traction and contact state.
    fn compute_tangential_force(
        &self,
        ctx: &mut ResolutionContext<'_>,
        element: &ContactElement,
        force: &mut [Real],
    ) -> Result<()>;
    /// Local normal moduli added to `stiffness` (`nd × nd`, column-major).
    fn compute_normal_moduli(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        scratch: &mut Scratch,
    ) -> Result<()>;
    /// Local tangential moduli written to `stiffness` (the penalty
    /// resolutions: the consistent friction tangent `−∂f_T/∂u` by default,
    /// upstream's stick/slip moduli with
    /// [`ContactUpstreamCompat::friction_moduli`]; all zero when the slave
    /// has no friction state). This is the element block only (slave and
    /// current master facet DOFs): after a facet change on a deformable
    /// master the consistent tangent also couples to the previous facet's
    /// other nodes, which
    /// [`Resolution::compute_tangential_moduli_with_coupling`] returns and
    /// the model's assembly adds.
    fn compute_tangential_moduli(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        scratch: &mut Scratch,
    ) -> Result<()>;
    /// [`Resolution::compute_tangential_moduli`] plus the coupling of the
    /// tangential force to the nodes of the slave's **previous** master
    /// facet that are not nodes of the contact element (the slave changed
    /// facet since the history): their ids go to
    /// `coupling_nodes` and their columns to `coupling`, one `nd × dim`
    /// column-major block per node (`nd` = the element's local DOF count);
    /// returns the number of such nodes (port extension: upstream's moduli
    /// have no such columns). Default: no coupling.
    ///
    /// # Errors
    /// See [`Resolution::compute_tangential_moduli`].
    fn compute_tangential_moduli_with_coupling(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        scratch: &mut Scratch,
        coupling_nodes: &mut [Idx],
        coupling: &mut [Real],
    ) -> Result<usize> {
        let _ = (coupling_nodes, coupling);
        self.compute_tangential_moduli(ctx, element, stiffness, scratch)?;
        Ok(0)
    }
    /// Hook before a solve step (upstream `beforeSolveStep`, empty).
    fn before_solve_step(&mut self) -> Result<()> {
        Ok(())
    }
    /// Hook after a solve step (upstream `afterSolveStep`, empty).
    fn after_solve_step(&mut self, converged: bool) -> Result<()> {
        let _ = converged;
        Ok(())
    }
    /// Parse the resolution's input-file section.
    fn parse(&mut self, section: &akantu_parser::ParserSection, permissive: bool) -> Result<()>;
}

/// Pressure law of a penalty resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenaltyLaw {
    /// `penalty_linear`: `p = ε_n ⟨g⟩`.
    Linear,
    /// `penalty_quadratic`: `p = ε_n (⟨g⟩² + ⟨g⟩)`.
    Quadratic,
}

/// Penalty resolution (upstream `ResolutionPenalty` /
/// `ResolutionPenaltyQuadratic`).
#[derive(Clone, Debug)]
pub struct ResolutionPenalty {
    id: Id,
    law: PenaltyLaw,
    /// `name` (upstream `std::string`, parsable + readable).
    pub name: ByteString,
    /// `mu` (friction coefficient).
    pub mu: Real,
    /// `is_master_deformable`.
    pub is_master_deformable: bool,
    /// `epsilon_n` (normal penalty).
    pub epsilon_n: Real,
    /// `epsilon_t` (tangential penalty).
    pub epsilon_t: Real,
}

static PENALTY_PARAMS: &[ParamDescriptor<ResolutionPenalty>] = &[
    ParamDescriptor::new(
        "name",
        ParamKind::ByteString,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "",
        |s| ParamValue::ByteString(s.name.clone()),
        Some(|s, v| {
            s.name = v.into_byte_string()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "mu",
        ParamKind::Real,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::MODIFIABLE.0),
        "Friction Coefficient",
        |s| ParamValue::Real(s.mu),
        Some(|s, v| {
            s.mu = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "is_master_deformable",
        ParamKind::Bool,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "Is master surface deformable",
        |s| ParamValue::Bool(s.is_master_deformable),
        Some(|s, v| {
            s.is_master_deformable = v.as_bool()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "epsilon_n",
        ParamKind::Real,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::MODIFIABLE.0),
        "Normal penalty parameter",
        |s| ParamValue::Real(s.epsilon_n),
        Some(|s, v| {
            s.epsilon_n = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "epsilon_t",
        ParamKind::Real,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::MODIFIABLE.0),
        "Tangential penalty parameter",
        |s| ParamValue::Real(s.epsilon_t),
        Some(|s, v| {
            s.epsilon_t = v.as_real()?;
            Ok(())
        }),
    ),
];

impl ParameterRegistry for ResolutionPenalty {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        table_get(PENALTY_PARAMS, self, name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set(PENALTY_PARAMS, self, name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        table_names(PENALTY_PARAMS)
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        table_info(PENALTY_PARAMS, name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set_from_input(PENALTY_PARAMS, self, name, value)
    }
}

impl Parsable for ResolutionPenalty {
    fn section_type(&self) -> ParserType {
        ParserType::ContactResolution
    }
    fn parsable_id(&self) -> &str {
        &self.id
    }
}

/// Copy of a nodal tuple into a stack buffer.
fn row<const N: usize>(a: &Array<Real>, node: usize) -> Result<[Real; N]> {
    let t = a.get_tuple(node)?;
    let mut out = [0.0; N];
    out.get_mut(..t.len())
        .ok_or_else(|| Error::invalid("nodal tuple longer than expected"))?
        .copy_from_slice(t);
    Ok(out)
}

/// Inverse of an `sd × sd` matrix (`sd ≤ 2`, Eigen's dynamic inverse);
/// a singular matrix is an error naming the contact element.
fn inverse_sd(sd: usize, mm: &[[Real; 2]; 2], element: &ContactElement) -> Result<[[Real; 2]; 2]> {
    let mut flat = [0.0; 4];
    for i in 0..sd {
        for j in 0..sd {
            flat[i + j * sd] = mm[i][j];
        }
    }
    let mut out = [0.0; 4];
    if crate::eigen_ops::small_inverse(sd, &flat[..sd * sd], &mut out[..sd * sd]) {
        return Err(Error::Numerical(format!(
            "contact element of slave {}: singular surface metric",
            element.slave
        )));
    }
    let mut r = [[0.0; 2]; 2];
    for i in 0..sd {
        for j in 0..sd {
            r[i][j] = out[i + j * sd];
        }
    }
    Ok(r)
}

/// Node coordinates of a master facet from `positions` into `out`
/// (column-major `dim × nn`; upstream `extractNodalValuesFromElement`),
/// without allocating. Returns `dim · nn`.
fn facet_coords(
    mesh: &Mesh,
    positions: &Array<Real>,
    element: akantu_core::Element,
    out: &mut [Real; 3 * MAX_FACET_NODES],
) -> Result<usize> {
    let conn = mesh.element_connectivity(element)?;
    let dim = positions.nb_component();
    if conn.len() * dim > out.len() {
        return Err(Error::UnsupportedElementType {
            ty: element.ty(),
            context: "contact master facet coordinates",
        });
    }
    for (k, &n) in conn.iter().enumerate() {
        out[k * dim..(k + 1) * dim].copy_from_slice(positions.get_tuple(try_to_usize(n)?)?);
    }
    Ok(conn.len() * dim)
}

impl ResolutionPenalty {
    /// New penalty resolution (parameters at their upstream defaults:
    /// `mu = epsilon_n = epsilon_t = 0`, rigid master, empty name).
    #[must_use]
    pub fn new(id: impl Into<Id>, law: PenaltyLaw) -> Self {
        Self {
            id: id.into(),
            law,
            name: ByteString::default(),
            mu: 0.0,
            is_master_deformable: false,
            epsilon_n: 0.0,
            epsilon_t: 0.0,
        }
    }

    /// Pressure law.
    #[must_use]
    pub fn law(&self) -> PenaltyLaw {
        self.law
    }

    /// Upstream `computeNormalTraction(gap)` (virtual).
    #[must_use]
    pub fn normal_traction(&self, gap: Real) -> Real {
        match self.law {
            PenaltyLaw::Linear => self.epsilon_n * macaulay(gap),
            PenaltyLaw::Quadratic => {
                self.epsilon_n * (macaulay(gap) * macaulay(gap) + macaulay(gap))
            }
        }
    }

    /// `‖t‖ = sqrt(Σ_ij t_i t_j a^{ij})` accumulated as upstream.
    fn traction_norm(t: &[Real], cmt: &[Real], sd: usize) -> Real {
        let mut n = 0.0;
        for i in 0..sd {
            for j in 0..sd {
                n += t[i] * t[j] * cmt[i + j * sd];
            }
        }
        n.sqrt()
    }

    /// Upstream `computeTrialTangentialTraction` (Schweizerhof variant):
    /// `traction` must be zero on entry (UD-001).
    fn trial_tangential_traction(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        current_tangent: &[Real],
        traction: &mut [Real],
    ) -> Result<()> {
        let dim = ctx.dim;
        let sd = ctx.sd();
        let s = try_to_usize(element.slave)?;
        let f = &*ctx.fields;
        let current_projection: [Real; 2] = row(&f.projections, s)?;
        let previous_projection: [Real; 2] = row(&f.previous_projections, s)?;
        let mut cov = [0.0; SMALL_MAX * SMALL_MAX];
        gram(current_tangent, dim, sd, &mut cov[..sd * sd])?;
        let previous_traction: [Real; 2] = row(&f.previous_tangential_tractions, s)?;
        let previous_tangent: [Real; 6] = row(&f.previous_tangents, s)?;
        let mut prev_cmt = [0.0; SMALL_MAX * SMALL_MAX];
        inverse_gram(
            &previous_tangent[..dim * sd],
            dim,
            sd,
            &mut prev_cmt[..sd * sd],
        )?;
        for alpha in 0..sd {
            for gamma in 0..sd {
                for beta in 0..sd {
                    let t_ab = dot(
                        &previous_tangent[beta * dim..(beta + 1) * dim],
                        &current_tangent[alpha * dim..(alpha + 1) * dim],
                    );
                    traction[alpha] +=
                        previous_traction[gamma] * prev_cmt[gamma + beta * sd] * t_ab;
                }
            }
        }
        let previous_element = *f
            .previous_master_elements
            .get_tuple(s)?
            .first()
            .ok_or_else(|| Error::invalid("previous_master_elements has no component"))?;
        let mut prev_buf = [0.0; 3 * MAX_FACET_NODES];
        let np = facet_coords(ctx.mesh, ctx.positions, previous_element, &mut prev_buf)?;
        let prev_coords = &prev_buf[..np];
        let mut previous_real = [0.0; 3];
        real_projection(
            prev_coords,
            dim,
            previous_element.ty(),
            &previous_projection[..sd],
            &mut previous_real,
        )?;
        let mut current_real = [0.0; 3];
        if ctx.compat.trial_slip_on_previous_element {
            if previous_element.ty().nb_nodes_per_element()
                != element.master.ty().nb_nodes_per_element()
            {
                return Err(Error::Unsupported(format!(
                    "UD-365 compatibility: the previous master {previous_element} and the current master {} \
                     have different node counts (undefined behaviour upstream)",
                    element.master
                )));
            }
            real_projection(
                prev_coords,
                dim,
                element.master.ty(),
                &current_projection[..sd],
                &mut current_real,
            )?;
        } else {
            let mut cur_buf = [0.0; 3 * MAX_FACET_NODES];
            let nc = facet_coords(ctx.mesh, ctx.positions, element.master, &mut cur_buf)?;
            real_projection(
                &cur_buf[..nc],
                dim,
                element.master.ty(),
                &current_projection[..sd],
                &mut current_real,
            )?;
        }
        let mut increment_real = [0.0; 3];
        for i in 0..dim {
            increment_real[i] = current_real[i] - previous_real[i];
        }
        let mut increment_xi = [0.0; 2];
        let mut cmt = [0.0; SMALL_MAX * SMALL_MAX];
        inverse_gram(current_tangent, dim, sd, &mut cmt[..sd * sd])?;
        for beta in 0..sd {
            for gamma in 0..sd {
                let mut temp = dot(
                    &increment_real[..dim],
                    &current_tangent[gamma * dim..(gamma + 1) * dim],
                );
                temp *= cmt[beta + gamma * sd];
                increment_xi[beta] += temp;
            }
        }
        // traction -= epsilon_t * covariant_metric_tensor * increment_xi
        // (product evaluated into a temporary: aliasing assumption)
        let mut tmp = [0.0; 2];
        if sd == 1 {
            // run-time 1 × 1 result: inner-product fallback on the unextracted lhs
            tmp[0] = 0.0;
            tmp[0] += (self.epsilon_t * cov[0]) * increment_xi[0];
        } else {
            for (i, t) in tmp.iter_mut().enumerate().take(sd) {
                let c = sum_from_zero(sd, &|l| cov[i + l * sd] * increment_xi[l]);
                *t = 0.0;
                *t += self.epsilon_t * c;
            }
        }
        for i in 0..sd {
            traction[i] -= tmp[i];
        }
        Ok(())
    }

    /// Upstream `computeTangentialTraction` (return map): writes the
    /// slave's tangential traction and contact state.
    fn tangential_traction(
        &self,
        ctx: &mut ResolutionContext<'_>,
        element: &ContactElement,
        covariant_basis: &[Real],
    ) -> Result<()> {
        let dim = ctx.dim;
        let sd = ctx.sd();
        let s = try_to_usize(element.slave)?;
        let gap = ctx.fields.gaps.read()?.get_tuple(s)?[0];
        let mut trial = [0.0; 2];
        self.trial_tangential_traction(ctx, element, covariant_basis, &mut trial)?;
        let mut cmt = [0.0; SMALL_MAX * SMALL_MAX];
        inverse_gram(covariant_basis, dim, sd, &mut cmt[..sd * sd])?;
        let trial_norm = Self::traction_norm(&trial, &cmt, sd);
        let p_n = self.normal_traction(gap);
        let stick = trial_norm <= self.mu * p_n;
        if stick {
            ctx.fields.contact_state.get_tuple_mut(s)?[0] = ContactState::Stick;
            let out = ctx.fields.tangential_tractions.get_tuple_mut(s)?;
            out.copy_from_slice(&trial[..sd]);
        } else {
            ctx.fields.contact_state.get_tuple_mut(s)?[0] = ContactState::Slip;
            // computeSlipTangentialTraction: norm recomputed identically
            let n = Self::traction_norm(&trial, &cmt, sd);
            let out = ctx.fields.tangential_tractions.get_tuple_mut(s)?;
            for (o, t) in out.iter_mut().zip(&trial) {
                *o = t / n * self.mu * p_n;
            }
        }
        Ok(())
    }

    /// Upstream normal moduli (**UD-368** compatibility; shared by both
    /// laws, `law` selects the factors and the UD-008 dyad).
    fn normal_moduli(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        w: &mut Scratch,
    ) -> Result<()> {
        let dim = ctx.dim;
        let sd = ctx.sd();
        let s = try_to_usize(element.slave)?;
        let f = &*ctx.fields;
        let gap = f.gaps.read()?.get_tuple(s)?[0];
        let projection: [Real; 2] = row(&f.projections, s)?;
        let nodal_area = f.nodal_area.read()?.get_tuple(s)?[0];
        let normal: [Real; 3] = row(&*f.normals.read()?, s)?;
        let covariant_basis: [Real; 6] = row(&f.tangents, s)?;
        let ty = element.master.ty();
        let nd = shape_function_matrix(ty, dim, &projection[..sd], &mut w.a)?;
        let (a, t1, p) = (&w.a[..dim * nd], &mut w.t1, &mut w.p);

        // k_main = (Aᵀ (n⊗n) A) * ε_n * H(g) [* (2g + 1)] * A_s
        let mut nn = [0.0; 9];
        outer(&normal[..dim], &normal[..dim], &mut nn[..dim * dim]);
        matmul(
            a,
            dim,
            nd,
            true,
            &nn[..dim * dim],
            dim,
            dim,
            false,
            &mut t1[..nd * dim],
        )?;
        matmul(
            &t1[..nd * dim],
            nd,
            dim,
            false,
            a,
            dim,
            nd,
            false,
            &mut p[..nd * nd],
        )?;
        let h = heaviside(gap);
        let k_main = &mut w.m1[..nd * nd];
        for (k, &v) in k_main.iter_mut().zip(p[..nd * nd].iter()) {
            *k = match self.law {
                PenaltyLaw::Linear => v * self.epsilon_n * h * nodal_area,
                PenaltyLaw::Quadratic => v * self.epsilon_n * h * (2. * gap + 1.) * nodal_area,
            };
        }

        let mut cmt = [0.0; SMALL_MAX * SMALL_MAX];
        inverse_gram(&covariant_basis[..dim * sd], dim, sd, &mut cmt[..sd * sd])?;
        w.m2[..nd * nd].fill(0.0);
        w.m3[..nd * nd].fill(0.0);
        derivative_shape_function_matrices(
            ty,
            dim,
            &projection[..sd],
            ctx.compat.derivative_shape_chunks,
            &mut w.ajs,
        )?;
        let quad_upstream_dyad =
            self.law == PenaltyLaw::Quadratic && ctx.compat.quadratic_rotational_dyad;
        for alpha in 0..sd {
            let tangent = &covariant_basis[alpha * dim..(alpha + 1) * dim];
            let mut n_outer_t = [0.0; 9];
            outer(&normal[..dim], tangent, &mut n_outer_t[..dim * dim]);
            let mut t_outer_n = [0.0; 9];
            outer(tangent, &normal[..dim], &mut t_outer_n[..dim * dim]);
            let second = if quad_upstream_dyad {
                n_outer_t
            } else {
                t_outer_n
            };
            for beta in 0..sd {
                let aj = &w.ajs[beta * dim * nd..(beta + 1) * dim * nd];
                let c = cmt[alpha + beta * sd];
                // k_rot1 += (A_jᵀ (n⊗τ) A) * a^{αβ}
                matmul(
                    aj,
                    dim,
                    nd,
                    true,
                    &n_outer_t[..dim * dim],
                    dim,
                    dim,
                    false,
                    &mut w.t1[..nd * dim],
                )?;
                matmul(
                    &w.t1[..nd * dim],
                    nd,
                    dim,
                    false,
                    &w.a[..dim * nd],
                    dim,
                    nd,
                    false,
                    &mut w.p[..nd * nd],
                )?;
                for (k, &v) in w.m2[..nd * nd].iter_mut().zip(w.p[..nd * nd].iter()) {
                    *k += v * c;
                }
                // k_rot2 += (Aᵀ (τ⊗n) A_j) * a^{αβ}
                matmul(
                    &w.a[..dim * nd],
                    dim,
                    nd,
                    true,
                    &second[..dim * dim],
                    dim,
                    dim,
                    false,
                    &mut w.t1[..nd * dim],
                )?;
                matmul(
                    &w.t1[..nd * dim],
                    nd,
                    dim,
                    false,
                    aj,
                    dim,
                    nd,
                    false,
                    &mut w.p[..nd * nd],
                )?;
                for (k, &v) in w.m3[..nd * nd].iter_mut().zip(w.p[..nd * nd].iter()) {
                    *k += v * c;
                }
            }
        }
        let factor = match self.law {
            PenaltyLaw::Linear => -self.epsilon_n * h * gap * nodal_area,
            PenaltyLaw::Quadratic => -self.epsilon_n * h * (gap * gap + gap) * nodal_area,
        };
        for k in w.m2[..nd * nd].iter_mut() {
            *k *= factor;
        }
        for k in w.m3[..nd * nd].iter_mut() {
            *k *= factor;
        }
        for (i, k) in stiffness[..nd * nd].iter_mut().enumerate() {
            *k += w.m1[i] + w.m2[i] + w.m3[i];
        }
        Ok(())
    }

    /// Derivative `p'(g)` of the pressure law (`ε_n H(g)`, quadratic
    /// `ε_n H(g)(2⟨g⟩ + 1)`).
    #[must_use]
    pub fn normal_traction_derivative(&self, gap: Real) -> Real {
        match self.law {
            PenaltyLaw::Linear => self.epsilon_n * heaviside(gap),
            PenaltyLaw::Quadratic => self.epsilon_n * heaviside(gap) * (2. * macaulay(gap) + 1.),
        }
    }

    /// Consistent normal moduli (**UD-368** correction): `K = −∂f_N/∂u` of
    /// `f_N = A_s p(g) Aᵀn` with the nodal area `A_s` frozen,
    ///
    /// `K = A_s [ p′ N Nᵀ + p ( T H⁻¹ N̄ᵀ + N̄ H⁻¹ Tᵀ − g N̄ H⁻¹ N̄ᵀ + T m⁻¹κH⁻¹ Tᵀ ) ]`
    ///
    /// with `N = Aᵀn`, `N̄_α = A_αᵀ n`, `T_α = Aᵀ a_α`, the natural
    /// (unnormalised) tangents `a_α = X ∂_αN`, `m_αβ = a_α·a_β`, the
    /// curvature `κ_αβ = n · X ∂²_αβN` and `H = m + g κ` (derivation:
    /// closest-point condition `a_α · (x_s − x_m) = 0`, `x_s − x_m = −g n`).
    /// Symmetric by construction (the force derives from the potential
    /// `A_s ∫p(g)`).
    #[allow(clippy::needless_range_loop)] // tensor-index loops transcribe the formulas
    fn consistent_normal_moduli(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        w: &mut Scratch,
    ) -> Result<()> {
        let dim = ctx.dim;
        let sd = ctx.sd();
        let s = try_to_usize(element.slave)?;
        let f = &*ctx.fields;
        let gap = f.gaps.read()?.get_tuple(s)?[0];
        let projection: [Real; 2] = row(&f.projections, s)?;
        let area = f.nodal_area.read()?.get_tuple(s)?[0];
        let normal: [Real; 3] = row(&*f.normals.read()?, s)?;
        let ty = element.master.ty();
        let nn = ty.nb_nodes_per_element();
        let xi = &projection[..sd];
        let mut coords_buf = [0.0; 3 * MAX_FACET_NODES];
        let ncoords = facet_coords(ctx.mesh, ctx.positions, element.master, &mut coords_buf)?;
        let coords = &coords_buf[..ncoords];
        let nd = shape_function_matrix(ty, dim, xi, &mut w.a)?;
        derivative_shape_function_matrices(ty, dim, xi, false, &mut w.ajs)?;
        // natural tangents, metric, curvature
        let mut dn = [0.0; 2 * MAX_FACET_NODES];
        dnds_at(ty, xi, &mut dn[..sd * nn])?;
        let mut a = [[0.0; 3]; 2];
        for (alpha, aa) in a.iter_mut().enumerate().take(sd) {
            for (i, v) in aa.iter_mut().enumerate().take(dim) {
                *v = (0..nn)
                    .map(|k| coords[i + k * dim] * dn[alpha + k * sd])
                    .sum();
            }
        }
        let mut m = [[0.0; 2]; 2];
        for alpha in 0..sd {
            for beta in 0..sd {
                m[alpha][beta] = dot(&a[alpha][..dim], &a[beta][..dim]);
            }
        }
        let mut d2 = [0.0; 4 * MAX_FACET_NODES];
        crate::geometry::d2nds2_at(ty, xi, &mut d2[..sd * sd * nn])?;
        let mut kappa = [[0.0; 2]; 2];
        for alpha in 0..sd {
            for beta in 0..sd {
                let idx = alpha * sd + beta;
                let mut x_ab = [0.0; 3];
                for (i, v) in x_ab.iter_mut().enumerate().take(dim) {
                    *v = (0..nn)
                        .map(|k| coords[i + k * dim] * d2[idx + k * sd * sd])
                        .sum();
                }
                kappa[alpha][beta] = dot(&x_ab[..dim], &normal[..dim]);
            }
        }
        let inv = |mm: &[[Real; 2]; 2]| -> Result<[[Real; 2]; 2]> {
            let mut flat = [0.0; 4];
            for i in 0..sd {
                for j in 0..sd {
                    flat[i + j * sd] = mm[i][j];
                }
            }
            let mut out = [0.0; 4];
            if crate::eigen_ops::small_inverse(sd, &flat[..sd * sd], &mut out[..sd * sd]) {
                return Err(Error::Numerical(format!(
                    "contact element of slave {}: singular surface metric",
                    element.slave
                )));
            }
            let mut r = [[0.0; 2]; 2];
            for i in 0..sd {
                for j in 0..sd {
                    r[i][j] = out[i + j * sd];
                }
            }
            Ok(r)
        };
        let minv = inv(&m)?;
        let mut h = [[0.0; 2]; 2];
        for alpha in 0..sd {
            for beta in 0..sd {
                h[alpha][beta] = m[alpha][beta] + gap * kappa[alpha][beta];
            }
        }
        let hinv = inv(&h)?;
        // c = m⁻¹ κ H⁻¹
        let mut c = [[0.0; 2]; 2];
        for alpha in 0..sd {
            for beta in 0..sd {
                let mut v = 0.0;
                for g1 in 0..sd {
                    for g2 in 0..sd {
                        v += minv[alpha][g1] * kappa[g1][g2] * hinv[g2][beta];
                    }
                }
                c[alpha][beta] = v;
            }
        }
        // N, N̄_α, T_α (nd each)
        let amat = &w.a[..dim * nd];
        let vec_t = |mat: &[Real], v: &[Real], i: usize| -> Real {
            (0..dim).map(|r| mat[r + i * dim] * v[r]).sum()
        };
        let mut nvec = [0.0; MAX_ELEMENT_DOFS];
        let mut nbar = [[0.0; MAX_ELEMENT_DOFS]; 2];
        let mut tvec = [[0.0; MAX_ELEMENT_DOFS]; 2];
        for i in 0..nd {
            nvec[i] = vec_t(amat, &normal[..dim], i);
            for alpha in 0..sd {
                let aj = &w.ajs[alpha * dim * nd..(alpha + 1) * dim * nd];
                nbar[alpha][i] = vec_t(aj, &normal[..dim], i);
                tvec[alpha][i] = vec_t(amat, &a[alpha][..dim], i);
            }
        }
        let p = self.normal_traction(gap);
        let dp = self.normal_traction_derivative(gap);
        for j in 0..nd {
            for i in 0..nd {
                let mut geo = 0.0;
                for alpha in 0..sd {
                    for beta in 0..sd {
                        geo += tvec[alpha][i] * hinv[alpha][beta] * nbar[beta][j]
                            + nbar[alpha][i] * hinv[alpha][beta] * tvec[beta][j]
                            - gap * nbar[alpha][i] * hinv[alpha][beta] * nbar[beta][j]
                            + tvec[alpha][i] * c[alpha][beta] * tvec[beta][j];
                    }
                }
                stiffness[i + j * nd] += area * (dp * nvec[i] * nvec[j] + p * geo);
            }
        }
        Ok(())
    }

    /// Consistent friction moduli (**UD-369** correction, the specification
    /// of): `K_T = −∂f_T/∂u` of the frictional force
    /// `f_T = A_s Σ_{αβ} t_β m̂^{αβ} Aᵀτ_α` with the history (`t^p`, `τ^p`,
    /// `ξ_p`, the previous master facet `e_p`) and the nodal area `A_s`
    /// frozen, in stick and in slip, linearised along each element DOF
    /// (forward mode; stack workspaces only):
    ///
    /// * `δξ = H⁻¹ (T − g N̄)·δu` (closest point, as the UD-368 tangent);
    /// * `δτ_α = s_α (I − τ_α τ_αᵀ) δa_α / |a_α|` with the natural tangents
    ///   `a_α = X ∂_αN`, `δa_α = Σ_k ∂_αN_k δX_k + x_{,αβ} δξ^β` and the
    ///   orientation `s_α = ±1` chosen by the detection;
    /// * `δm̂_αβ = δτ_α·τ_β + τ_α·δτ_β`, `δm̂⁻¹ = −m̂⁻¹ δm̂ m̂⁻¹`;
    /// * trial traction `t^tr_α = t^p_γ m̂_p^{γβ}(τ^p_β·τ_α) − ε_t τ_α·Δx`,
    ///   `δt^tr_α = t^p_γ m̂_p^{γβ}(τ^p_β·δτ_α) − ε_t(δτ_α·Δx + τ_α·δΔx)`,
    ///   `Δx = x_c(ξ) − x_{e_p}(ξ_p)` (the current point on the current
    ///   facet, or on `e_p` with the UD-365 switch); when the slave changed
    ///   facet, the nodes of `e_p` that are not nodes of the element couple
    ///   to DOFs outside the element: their columns (`δΔx = c_k δX_k`,
    ///   `c_k = −N_k(ξ_p)`, plus `N_k(ξ)` when the current point is on
    ///   `e_p`; nothing else depends on them) go to `coupling` when it is
    ///   given (dropped otherwise);
    /// * stick: `δt = δt^tr`; slip (`t = μ p_N t^tr/q`, `q² = t^tr m̂⁻¹
    ///   t^tr`): `δt = μ[δp_N t^tr/q + p_N δt^tr/q − p_N t^tr δq/q²]`,
    ///   `δq = (t^tr m̂⁻¹ δt^tr + ½ t^tr δm̂⁻¹ t^tr)/q`, `δp_N = −p′(g) N·δu`;
    /// * `δf_T = A_s Σ_{αβ}[δt_β m̂^{αβ} Aᵀτ_α + t_β δm̂^{αβ} Aᵀτ_α + t_β m̂^{αβ}
    ///   (Aᵀδτ_α − Σ_γ A_γᵀτ_α δξ^γ)]`.
    ///
    /// The tangent is unsymmetric. It reads the contact state written by
    /// the force computation (no state: zero, as upstream) and recomputes
    /// the trial traction exactly as the force did.
    #[allow(clippy::needless_range_loop, clippy::too_many_lines)] // tensor-index loops transcribe the formulas
    fn consistent_friction_moduli(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        coupling: Option<(&mut [Idx], &mut [Real])>,
    ) -> Result<usize> {
        let dim = ctx.dim;
        let sd = ctx.sd();
        let s = try_to_usize(element.slave)?;
        let f = &*ctx.fields;
        let stick = match f.contact_state.get_tuple(s)?[0] {
            ContactState::NoContact => return Ok(0),
            ContactState::Stick => true,
            ContactState::Slip => false,
        };
        let previous_element = *f
            .previous_master_elements
            .get_tuple(s)?
            .first()
            .ok_or_else(|| Error::invalid("previous_master_elements has no component"))?;
        if previous_element.is_null() {
            return Ok(0);
        }
        let gap = f.gaps.read()?.get_tuple(s)?[0];
        let projection: [Real; 2] = row(&f.projections, s)?;
        let previous_projection: [Real; 2] = row(&f.previous_projections, s)?;
        let area = f.nodal_area.read()?.get_tuple(s)?[0];
        let normal: [Real; 3] = row(&*f.normals.read()?, s)?;
        let tau_buf: [Real; 6] = row(&f.tangents, s)?;
        let tp: [Real; 2] = row(&f.previous_tangential_tractions, s)?;
        let taup_buf: [Real; 6] = row(&f.previous_tangents, s)?;
        let tau = |a: usize| &tau_buf[a * dim..(a + 1) * dim];
        let taup = |a: usize| &taup_buf[a * dim..(a + 1) * dim];
        let ty = element.master.ty();
        let nn = ty.nb_nodes_per_element();
        let nd = dim * (nn + 1);
        if stiffness.len() < nd * nd {
            return Err(Error::DimensionMismatch {
                context: "consistent friction moduli: local stiffness",
                expected: nd * nd,
                got: stiffness.len(),
            });
        }
        let xi = &projection[..sd];
        // current element: coordinates, shape functions and derivatives
        let mut coords_buf = [0.0; 3 * MAX_FACET_NODES];
        let nc = facet_coords(ctx.mesh, ctx.positions, element.master, &mut coords_buf)?;
        let coords = &coords_buf[..nc];
        let mut nf = [0.0; MAX_FACET_NODES];
        shapes_at(ty, xi, &mut nf[..nn])?;
        let mut dn = [0.0; 2 * MAX_FACET_NODES];
        dnds_at(ty, xi, &mut dn[..sd * nn])?;
        let mut d2 = [0.0; 4 * MAX_FACET_NODES];
        crate::geometry::d2nds2_at(ty, xi, &mut d2[..sd * sd * nn])?;
        // natural tangents a_α, second derivatives x_{,αβ}
        let mut a = [[0.0; 3]; 2];
        let mut xab = [[[0.0; 3]; 2]; 2];
        for al in 0..sd {
            for i in 0..dim {
                a[al][i] = (0..nn).map(|k| coords[i + k * dim] * dn[al + k * sd]).sum();
                for be in 0..sd {
                    xab[al][be][i] = (0..nn)
                        .map(|k| coords[i + k * dim] * d2[al * sd + be + k * sd * sd])
                        .sum();
                }
            }
        }
        let mut m = [[0.0; 2]; 2];
        let mut kappa = [[0.0; 2]; 2];
        let mut mh = [[0.0; 2]; 2];
        let mut mhp = [[0.0; 2]; 2];
        for al in 0..sd {
            for be in 0..sd {
                m[al][be] = dot(&a[al][..dim], &a[be][..dim]);
                kappa[al][be] = dot(&xab[al][be][..dim], &normal[..dim]);
                mh[al][be] = dot(tau(al), tau(be));
                mhp[al][be] = dot(taup(al), taup(be));
            }
        }
        let mut h = [[0.0; 2]; 2];
        for al in 0..sd {
            for be in 0..sd {
                h[al][be] = m[al][be] + gap * kappa[al][be];
            }
        }
        let hinv = inverse_sd(sd, &h, element)?;
        let mhi = inverse_sd(sd, &mh, element)?;
        let mhpi = inverse_sd(sd, &mhp, element)?;
        // |a_α| and the orientation s_α of the stored unit tangents
        let mut alen = [0.0; 2];
        let mut sgn = [1.0; 2];
        for al in 0..sd {
            alen[al] = norm(&a[al][..dim]);
            if dot(tau(al), &a[al][..dim]) < 0.0 {
                sgn[al] = -1.0;
            }
        }
        // the trial traction exactly as the force computed it
        let mut ttr = [0.0; 2];
        self.trial_tangential_traction(ctx, element, &tau_buf[..dim * sd], &mut ttr)?;
        let mut cmt = [0.0; SMALL_MAX * SMALL_MAX];
        inverse_gram(&tau_buf[..dim * sd], dim, sd, &mut cmt[..sd * sd])?;
        let q = Self::traction_norm(&ttr, &cmt, sd);
        let p_n = self.normal_traction(gap);
        let dp_n = self.normal_traction_derivative(gap);
        let mut t = [0.0; 2];
        for al in 0..sd {
            t[al] = if stick {
                ttr[al]
            } else {
                ttr[al] / q * self.mu * p_n
            };
        }
        // Δx = x_c(ξ) − x_{e_p}(ξ_p), the facet of the current point and the
        // local DOF block of each node of both facets (None: outside)
        let local_block = |node: Idx| -> Result<Option<usize>> {
            if node == element.slave {
                return Ok(Some(0));
            }
            let conn = ctx.mesh.element_connectivity(element.master)?;
            Ok(conn.iter().position(|&c| c == node).map(|k| k + 1))
        };
        let pty = previous_element.ty();
        let npn = pty.nb_nodes_per_element();
        let mut pcoords_buf = [0.0; 3 * MAX_FACET_NODES];
        let npc = facet_coords(ctx.mesh, ctx.positions, previous_element, &mut pcoords_buf)?;
        let pcoords = &pcoords_buf[..npc];
        let mut npf = [0.0; MAX_FACET_NODES];
        shapes_at(pty, &previous_projection[..sd], &mut npf[..npn])?;
        let mut pblock = [None; MAX_FACET_NODES];
        for (k, &node) in ctx
            .mesh
            .element_connectivity(previous_element)?
            .iter()
            .enumerate()
        {
            pblock[k] = local_block(node)?;
        }
        // current point: on the current facet (default) or on e_p (UD-365)
        let on_previous = ctx.compat.trial_slip_on_previous_element;
        let (cty, ccoords) = if on_previous {
            (pty, pcoords)
        } else {
            (ty, coords)
        };
        let ncn = cty.nb_nodes_per_element();
        let mut ncf = [0.0; MAX_FACET_NODES];
        shapes_at(cty, xi, &mut ncf[..ncn])?;
        let mut dnc = [0.0; 2 * MAX_FACET_NODES];
        dnds_at(cty, xi, &mut dnc[..sd * ncn])?;
        let mut ac = [[0.0; 3]; 2];
        for al in 0..sd {
            for i in 0..dim {
                ac[al][i] = (0..ncn)
                    .map(|k| ccoords[i + k * dim] * dnc[al + k * sd])
                    .sum();
            }
        }
        let mut cblock = [None; MAX_FACET_NODES];
        if on_previous {
            cblock[..npn].copy_from_slice(&pblock[..npn]);
        } else {
            for k in 0..nn {
                cblock[k] = Some(k + 1);
            }
        }
        let mut dx = [0.0; 3];
        for i in 0..dim {
            let xc: Real = (0..ncn).map(|k| ncf[k] * ccoords[i + k * dim]).sum();
            let xp: Real = (0..npn).map(|k| npf[k] * pcoords[i + k * dim]).sum();
            dx[i] = xc - xp;
        }
        // row vectors over the element DOFs: N = Aᵀn, T_α = Aᵀa_α,
        // N̄_α = A_αᵀn, δξ^α = H⁻¹(T − g N̄)
        let mut nvec = [0.0; MAX_ELEMENT_DOFS];
        let mut dxi_row = [[0.0; MAX_ELEMENT_DOFS]; 2];
        {
            let mut tvec = [[0.0; MAX_ELEMENT_DOFS]; 2];
            let mut nbar = [[0.0; MAX_ELEMENT_DOFS]; 2];
            for i in 0..dim {
                nvec[i] = normal[i];
                for al in 0..sd {
                    tvec[al][i] = a[al][i];
                }
            }
            for k in 0..nn {
                for i in 0..dim {
                    let r = dim * (k + 1) + i;
                    nvec[r] = -nf[k] * normal[i];
                    for al in 0..sd {
                        tvec[al][r] = -nf[k] * a[al][i];
                        nbar[al][r] = dn[al + k * sd] * normal[i];
                    }
                }
            }
            for al in 0..sd {
                for r in 0..nd {
                    dxi_row[al][r] = (0..sd)
                        .map(|be| hinv[al][be] * (tvec[be][r] - gap * nbar[be][r]))
                        .sum();
                }
            }
        }
        // Aᵀτ_α (constant over the columns)
        let mut atau = [[0.0; MAX_ELEMENT_DOFS]; 2];
        for al in 0..sd {
            for i in 0..dim {
                atau[al][i] = tau(al)[i];
                for k in 0..nn {
                    atau[al][dim * (k + 1) + i] = -nf[k] * tau(al)[i];
                }
            }
        }
        // transported previous traction coefficients c_β = t^p_γ m̂_p^{γβ}
        let mut cp = [0.0; 2];
        for be in 0..sd {
            cp[be] = (0..sd).map(|ga| tp[ga] * mhpi[ga][be]).sum();
        }
        let mut df = [0.0; MAX_ELEMENT_DOFS];
        for j in 0..nd {
            let (blk, comp) = (j / dim, j % dim);
            let dxi = [dxi_row[0][j], if sd > 1 { dxi_row[1][j] } else { 0.0 }];
            // δa_α, δτ_α
            let mut dta = [[0.0; 3]; 2];
            for al in 0..sd {
                let mut da = [0.0; 3];
                if blk > 0 {
                    da[comp] += dn[al + (blk - 1) * sd];
                }
                for be in 0..sd {
                    for i in 0..dim {
                        da[i] += xab[al][be][i] * dxi[be];
                    }
                }
                let c = dot(tau(al), &da[..dim]);
                for i in 0..dim {
                    dta[al][i] = sgn[al] * (da[i] - tau(al)[i] * c) / alen[al];
                }
            }
            let mut dmh = [[0.0; 2]; 2];
            for al in 0..sd {
                for be in 0..sd {
                    dmh[al][be] = dot(&dta[al][..dim], tau(be)) + dot(tau(al), &dta[be][..dim]);
                }
            }
            let mut dmhi = [[0.0; 2]; 2];
            for al in 0..sd {
                for be in 0..sd {
                    let mut v = 0.0;
                    for ga in 0..sd {
                        for th in 0..sd {
                            v -= mhi[al][ga] * dmh[ga][th] * mhi[th][be];
                        }
                    }
                    dmhi[al][be] = v;
                }
            }
            // δΔx
            let mut ddx = [0.0; 3];
            for k in 0..ncn {
                if cblock[k] == Some(blk) {
                    ddx[comp] += ncf[k];
                }
            }
            for k in 0..npn {
                if pblock[k] == Some(blk) {
                    ddx[comp] -= npf[k];
                }
            }
            for be in 0..sd {
                for i in 0..dim {
                    ddx[i] += ac[be][i] * dxi[be];
                }
            }
            // δt^tr, δt
            let mut dttr = [0.0; 2];
            for al in 0..sd {
                let transported: Real = (0..sd)
                    .map(|be| cp[be] * dot(taup(be), &dta[al][..dim]))
                    .sum();
                dttr[al] = transported
                    - self.epsilon_t
                        * (dot(&dta[al][..dim], &dx[..dim]) + dot(tau(al), &ddx[..dim]));
            }
            let mut dt = [0.0; 2];
            if stick {
                dt = dttr;
            } else {
                let mut dq = 0.0;
                for al in 0..sd {
                    for be in 0..sd {
                        dq += ttr[al] * mhi[al][be] * dttr[be]
                            + 0.5 * ttr[al] * dmhi[al][be] * ttr[be];
                    }
                }
                dq /= q;
                let dpn = -dp_n * nvec[j];
                for be in 0..sd {
                    dt[be] = self.mu
                        * (dpn * ttr[be] / q + p_n * dttr[be] / q - p_n * ttr[be] * dq / (q * q));
                }
            }
            // δf_T
            df[..nd].fill(0.0);
            for al in 0..sd {
                // δ(Aᵀτ_α): slave rows δτ_α, master rows −N_k δτ_α − (∂_γN_k δξ^γ) τ_α
                for be in 0..sd {
                    let c1 = dt[be] * mhi[al][be] + t[be] * dmhi[al][be];
                    let c2 = t[be] * mhi[al][be];
                    for i in 0..dim {
                        df[i] += area * (c1 * atau[al][i] + c2 * dta[al][i]);
                    }
                    for k in 0..nn {
                        let dnk: Real = (0..sd).map(|ga| dn[ga + k * sd] * dxi[ga]).sum();
                        for i in 0..dim {
                            let r = dim * (k + 1) + i;
                            let datau = -nf[k] * dta[al][i] - dnk * tau(al)[i];
                            df[r] += area * (c1 * atau[al][r] + c2 * datau);
                        }
                    }
                }
            }
            for r in 0..nd {
                stiffness[r + j * nd] -= df[r];
            }
        }
        // previous-facet nodes outside the element:
        // only δΔx = c_k δX_k is non-zero in their columns (δξ, δτ, δm̂,
        // δp_N vanish), so δt^tr_α = −ε_t c_k τ_α[comp] and the force
        // variation reduces to the δt terms
        let Some((nodes, block)) = coupling else {
            return Ok(0);
        };
        let pconn = ctx.mesh.element_connectivity(previous_element)?;
        let mut count = 0;
        for k in 0..npn {
            if pblock[k].is_some() {
                continue;
            }
            let ck = if on_previous { ncf[k] } else { 0.0 } - npf[k];
            if nodes.len() <= count || block.len() < (count + 1) * dim * nd {
                return Err(Error::DimensionMismatch {
                    context: "consistent friction moduli: previous-facet coupling buffer",
                    expected: (count + 1) * dim * nd,
                    got: block.len(),
                });
            }
            nodes[count] = pconn[k];
            for comp in 0..dim {
                let mut dttr = [0.0; 2];
                for al in 0..sd {
                    dttr[al] = -self.epsilon_t * (tau(al)[comp] * ck);
                }
                let mut dt = [0.0; 2];
                if stick {
                    dt = dttr;
                } else {
                    let mut dq = 0.0;
                    for al in 0..sd {
                        for be in 0..sd {
                            dq += ttr[al] * mhi[al][be] * dttr[be];
                        }
                    }
                    dq /= q;
                    for be in 0..sd {
                        dt[be] = self.mu * (p_n * dttr[be] / q - p_n * ttr[be] * dq / (q * q));
                    }
                }
                df[..nd].fill(0.0);
                for al in 0..sd {
                    for be in 0..sd {
                        let c1 = dt[be] * mhi[al][be];
                        for r in 0..nd {
                            df[r] += area * c1 * atau[al][r];
                        }
                    }
                }
                let col = &mut block[(count * dim + comp) * nd..(count * dim + comp + 1) * nd];
                for r in 0..nd {
                    col[r] = -df[r];
                }
            }
            count += 1;
        }
        Ok(count)
    }

    /// Upstream `computeStickModuli` with the `θ` loop over the tangents
    /// (UD-362; **UD-369** compatibility: these moduli are not the
    /// derivative of the tangential force, every term has the opposite
    /// sign).
    #[allow(clippy::needless_range_loop)] // tensor-index loops transcribe the formulas
    fn stick_moduli(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        w: &mut Scratch,
    ) -> Result<()> {
        let dim = ctx.dim;
        let sd = ctx.sd();
        let s = try_to_usize(element.slave)?;
        let f = &*ctx.fields;
        let projection: [Real; 2] = row(&f.projections, s)?;
        let nodal_area = f.nodal_area.read()?.get_tuple(s)?[0];
        let covariant_basis: [Real; 6] = row(&f.tangents, s)?;
        let tangential_traction: [Real; 2] = row(&f.tangential_tractions, s)?;
        let ty = element.master.ty();
        let nd = shape_function_matrix(ty, dim, &projection[..sd], &mut w.a)?;
        derivative_shape_function_matrices(
            ty,
            dim,
            &projection[..sd],
            ctx.compat.derivative_shape_chunks,
            &mut w.ajs,
        )?;
        let mut cmt = [0.0; SMALL_MAX * SMALL_MAX];
        inverse_gram(&covariant_basis[..dim * sd], dim, sd, &mut cmt[..sd * sd])?;
        let tan = |i: usize| &covariant_basis[i * dim..(i + 1) * dim];

        // k_main = −ε_t Σ (Aᵀ (τ_α⊗τ_β) A) a^{αβ}
        w.m1[..nd * nd].fill(0.0);
        for alpha in 0..sd {
            for beta in 0..sd {
                let mut tt = [0.0; 9];
                outer(tan(alpha), tan(beta), &mut tt[..dim * dim]);
                matmul(
                    &w.a[..dim * nd],
                    dim,
                    nd,
                    true,
                    &tt[..dim * dim],
                    dim,
                    dim,
                    false,
                    &mut w.t1[..nd * dim],
                )?;
                matmul(
                    &w.t1[..nd * dim],
                    nd,
                    dim,
                    false,
                    &w.a[..dim * nd],
                    dim,
                    nd,
                    false,
                    &mut w.p[..nd * nd],
                )?;
                let c = cmt[alpha + beta * sd];
                for (k, &v) in w.m1[..nd * nd].iter_mut().zip(w.p[..nd * nd].iter()) {
                    *k += v * c;
                }
            }
        }
        for k in w.m1[..nd * nd].iter_mut() {
            *k *= -self.epsilon_t;
        }

        // k_second = Σ_α t_α Σ_{β,γ,θ} [ Aᵀ(τ_γ⊗τ_θ)A_β a^{αθ}a^{βγ} + A_βᵀ(τ_γ⊗τ_θ)A a^{αγ}a^{βθ} ]
        w.m2[..nd * nd].fill(0.0);
        for alpha in 0..sd {
            w.m3[..nd * nd].fill(0.0);
            for beta in 0..sd {
                for gamma in 0..sd {
                    for theta in 0..sd {
                        let aj = &w.ajs[beta * dim * nd..(beta + 1) * dim * nd];
                        let mut tt = [0.0; 9];
                        outer(tan(gamma), tan(theta), &mut tt[..dim * dim]);
                        matmul(
                            &w.a[..dim * nd],
                            dim,
                            nd,
                            true,
                            &tt[..dim * dim],
                            dim,
                            dim,
                            false,
                            &mut w.t1[..nd * dim],
                        )?;
                        matmul(
                            &w.t1[..nd * dim],
                            nd,
                            dim,
                            false,
                            aj,
                            dim,
                            nd,
                            false,
                            &mut w.p[..nd * nd],
                        )?;
                        matmul(
                            aj,
                            dim,
                            nd,
                            true,
                            &tt[..dim * dim],
                            dim,
                            dim,
                            false,
                            &mut w.t1[..nd * dim],
                        )?;
                        matmul(
                            &w.t1[..nd * dim],
                            nd,
                            dim,
                            false,
                            &w.a[..dim * nd],
                            dim,
                            nd,
                            false,
                            &mut w.p2[..nd * nd],
                        )?;
                        let (c1, c2) = (cmt[alpha + theta * sd], cmt[beta + gamma * sd]);
                        let (c3, c4) = (cmt[alpha + gamma * sd], cmt[beta + theta * sd]);
                        for i in 0..nd * nd {
                            w.m3[i] += w.p[i] * c1 * c2 + w.p2[i] * c3 * c4;
                        }
                    }
                }
            }
            let ta = tangential_traction[alpha];
            for i in 0..nd * nd {
                w.m2[i] += ta * w.m3[i];
            }
        }
        for i in 0..nd * nd {
            stiffness[i] += w.m1[i] * nodal_area - w.m2[i] * nodal_area;
        }
        Ok(())
    }

    /// Upstream `computeSlipModuli` (with `k_third` zero-initialised,
    /// UD-001; **UD-369** compatibility: not the derivative of the slip
    /// force either — opposite signs, and the return-map term divides by the
    /// returned norm `μ p_N` instead of the trial norm, 3-D only).
    #[allow(clippy::needless_range_loop)] // tensor-index loops transcribe the formulas
    fn slip_moduli(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        w: &mut Scratch,
    ) -> Result<()> {
        let dim = ctx.dim;
        let sd = ctx.sd();
        let s = try_to_usize(element.slave)?;
        let f = &*ctx.fields;
        let gap = f.gaps.read()?.get_tuple(s)?[0];
        let nodal_area = f.nodal_area.read()?.get_tuple(s)?[0];
        let p_n = self.normal_traction(gap);
        let projection: [Real; 2] = row(&f.projections, s)?;
        let normal: [Real; 3] = row(&*f.normals.read()?, s)?;
        let covariant_basis: [Real; 6] = row(&f.tangents, s)?;
        let tt_s: [Real; 2] = row(&f.tangential_tractions, s)?;
        let ty = element.master.ty();
        let nd = shape_function_matrix(ty, dim, &projection[..sd], &mut w.a)?;
        derivative_shape_function_matrices(
            ty,
            dim,
            &projection[..sd],
            ctx.compat.derivative_shape_chunks,
            &mut w.ajs,
        )?;
        let mut cmt = [0.0; SMALL_MAX * SMALL_MAX];
        inverse_gram(&covariant_basis[..dim * sd], dim, sd, &mut cmt[..sd * sd])?;
        let tn = Self::traction_norm(&tt_s, &cmt, sd);
        let tn3 = tn.powf(3.0);
        let tan = |i: usize| &covariant_basis[i * dim..(i + 1) * dim];
        let (eps_n, eps_t, mu) = (self.epsilon_n, self.epsilon_t, self.mu);
        for m in [&mut w.m1, &mut w.m2, &mut w.m3, &mut w.m4] {
            m[..nd * nd].fill(0.0);
        }
        for alpha in 0..sd {
            for beta in 0..sd {
                let aj_range = beta * dim * nd..(beta + 1) * dim * nd;
                // eq. 107: k_first
                let mut t_outer_n = [0.0; 9];
                outer(tan(beta), &normal[..dim], &mut t_outer_n[..dim * dim]);
                matmul(
                    &w.a[..dim * nd],
                    dim,
                    nd,
                    true,
                    &t_outer_n[..dim * dim],
                    dim,
                    dim,
                    false,
                    &mut w.t1[..nd * dim],
                )?;
                matmul(
                    &w.t1[..nd * dim],
                    nd,
                    dim,
                    false,
                    &w.a[..dim * nd],
                    dim,
                    nd,
                    false,
                    &mut w.p[..nd * nd],
                )?;
                let c_ab = cmt[alpha + beta * sd];
                for (k, &v) in w.m1[..nd * nd].iter_mut().zip(w.p[..nd * nd].iter()) {
                    *k += v * eps_n * mu * tt_s[alpha] * c_ab / tn * nodal_area;
                }
                // eq. 107a: k_second
                let mut t_outer_t = [0.0; 9];
                outer(tan(alpha), tan(beta), &mut t_outer_t[..dim * dim]);
                matmul(
                    &w.a[..dim * nd],
                    dim,
                    nd,
                    true,
                    &t_outer_t[..dim * dim],
                    dim,
                    dim,
                    false,
                    &mut w.t1[..nd * dim],
                )?;
                matmul(
                    &w.t1[..nd * dim],
                    nd,
                    dim,
                    false,
                    &w.a[..dim * nd],
                    dim,
                    nd,
                    false,
                    &mut w.p[..nd * nd],
                )?;
                for (k, &v) in w.m2[..nd * nd].iter_mut().zip(w.p[..nd * nd].iter()) {
                    *k += v * eps_t * mu * p_n * c_ab / tn * nodal_area;
                }
                for gamma in 0..sd {
                    for theta in 0..sd {
                        outer(tan(gamma), tan(theta), &mut t_outer_t[..dim * dim]);
                        // eq. 107b: k_third
                        matmul(
                            &w.a[..dim * nd],
                            dim,
                            nd,
                            true,
                            &t_outer_t[..dim * dim],
                            dim,
                            dim,
                            false,
                            &mut w.t1[..nd * dim],
                        )?;
                        matmul(
                            &w.t1[..nd * dim],
                            nd,
                            dim,
                            false,
                            &w.a[..dim * nd],
                            dim,
                            nd,
                            false,
                            &mut w.p[..nd * nd],
                        )?;
                        let (c_ag, c_bt) = (cmt[alpha + gamma * sd], cmt[beta + theta * sd]);
                        let (c_at, c_bg) = (cmt[alpha + theta * sd], cmt[beta + gamma * sd]);
                        for (k, &v) in w.m3[..nd * nd].iter_mut().zip(w.p[..nd * nd].iter()) {
                            *k += v * eps_t * mu * p_n * tt_s[alpha] * tt_s[beta] * c_ag * c_bt
                                / tn3
                                * nodal_area;
                        }
                        // eq. 107c: k_fourth
                        matmul(
                            &w.t1[..nd * dim],
                            nd,
                            dim,
                            false,
                            &w.ajs[aj_range.clone()],
                            dim,
                            nd,
                            false,
                            &mut w.p[..nd * nd],
                        )?;
                        matmul(
                            &w.ajs[aj_range.clone()],
                            dim,
                            nd,
                            true,
                            &t_outer_t[..dim * dim],
                            dim,
                            dim,
                            false,
                            &mut w.t1[..nd * dim],
                        )?;
                        matmul(
                            &w.t1[..nd * dim],
                            nd,
                            dim,
                            false,
                            &w.a[..dim * nd],
                            dim,
                            nd,
                            false,
                            &mut w.p2[..nd * nd],
                        )?;
                        for i in 0..nd * nd {
                            w.m4[i] += (w.p[i] * c_at * c_bg + w.p2[i] * c_ag * c_bt)
                                * nodal_area
                                * mu
                                * p_n
                                * tt_s[alpha]
                                / tn;
                        }
                    }
                }
            }
        }
        for i in 0..nd * nd {
            stiffness[i] += w.m3[i] + w.m4[i] - w.m1[i] - w.m2[i];
        }
        Ok(())
    }
}

impl Resolution for ResolutionPenalty {
    fn id(&self) -> &str {
        &self.id
    }

    fn resolution_type(&self) -> &str {
        match self.law {
            PenaltyLaw::Linear => "penalty_linear",
            PenaltyLaw::Quadratic => "penalty_quadratic",
        }
    }

    fn mu(&self) -> Real {
        self.mu
    }

    fn is_master_deformable(&self) -> bool {
        self.is_master_deformable
    }

    fn compute_normal_force(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        force: &mut [Real],
    ) -> Result<()> {
        let dim = ctx.dim;
        let sd = ctx.sd();
        let s = try_to_usize(element.slave)?;
        let f = &*ctx.fields;
        let gap = f.gaps.read()?.get_tuple(s)?[0];
        let normal: [Real; 3] = row(&*f.normals.read()?, s)?;
        let projection: [Real; 2] = row(&f.projections, s)?;
        let p_n = self.normal_traction(gap) * f.nodal_area.read()?.get_tuple(s)?[0];
        let mut a = [0.0; 3 * MAX_ELEMENT_DOFS];
        let nd = shape_function_matrix(element.master.ty(), dim, &projection[..sd], &mut a)?;
        if force.len() != nd {
            return Err(Error::DimensionMismatch {
                context: "compute_normal_force: local force",
                expected: nd,
                got: force.len(),
            });
        }
        gemv_scaled_transposed(p_n, &a[..dim * nd], dim, nd, &normal[..dim], force);
        Ok(())
    }

    fn compute_tangential_force(
        &self,
        ctx: &mut ResolutionContext<'_>,
        element: &ContactElement,
        force: &mut [Real],
    ) -> Result<()> {
        force.fill(0.0);
        if self.mu == 0.0 {
            return Ok(());
        }
        let dim = ctx.dim;
        let sd = ctx.sd();
        let s = try_to_usize(element.slave)?;
        let projection: [Real; 2] = row(&ctx.fields.projections, s)?;
        let covariant_basis: [Real; 6] = row(&ctx.fields.tangents, s)?;
        if s >= ctx.fields.previous_master_elements.len() {
            return Ok(());
        }
        let previous = ctx.fields.previous_master_elements.get_tuple(s)?[0];
        if previous.is_null() {
            return Ok(());
        }
        self.tangential_traction(ctx, element, &covariant_basis[..dim * sd])?;
        let mut a = [0.0; 3 * MAX_ELEMENT_DOFS];
        let nd = shape_function_matrix(element.master.ty(), dim, &projection[..sd], &mut a)?;
        if force.len() != nd {
            return Err(Error::DimensionMismatch {
                context: "compute_tangential_force: local force",
                expected: nd,
                got: force.len(),
            });
        }
        let mut cmt = [0.0; SMALL_MAX * SMALL_MAX];
        inverse_gram(&covariant_basis[..dim * sd], dim, sd, &mut cmt[..sd * sd])?;
        let nodal_area = ctx.fields.nodal_area.read()?.get_tuple(s)?[0];
        let traction: [Real; 2] = row(&ctx.fields.tangential_tractions, s)?;
        let mut tmp = [0.0; MAX_ELEMENT_DOFS];
        for alpha in 0..sd {
            let tangent = &covariant_basis[alpha * dim..(alpha + 1) * dim];
            for beta in 0..sd {
                gemv_scaled_transposed(
                    traction[beta],
                    &a[..dim * nd],
                    dim,
                    nd,
                    tangent,
                    &mut tmp[..nd],
                );
                let c = cmt[alpha + beta * sd];
                for (fv, t) in force.iter_mut().zip(&tmp[..nd]) {
                    *fv += t * c * nodal_area;
                }
            }
        }
        Ok(())
    }

    fn compute_normal_moduli(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        scratch: &mut Scratch,
    ) -> Result<()> {
        if ctx.compat.normal_moduli {
            self.normal_moduli(ctx, element, stiffness, scratch)
        } else {
            self.consistent_normal_moduli(ctx, element, stiffness, scratch)
        }
    }

    fn compute_tangential_moduli(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        scratch: &mut Scratch,
    ) -> Result<()> {
        if self.mu == 0.0 {
            return Ok(());
        }
        stiffness.fill(0.0);
        if !ctx.compat.friction_moduli {
            return self
                .consistent_friction_moduli(ctx, element, stiffness, None)
                .map(|_| ());
        }
        let s = try_to_usize(element.slave)?;
        match ctx.fields.contact_state.get_tuple(s)?[0] {
            ContactState::Stick => self.stick_moduli(ctx, element, stiffness, scratch),
            ContactState::Slip => self.slip_moduli(ctx, element, stiffness, scratch),
            ContactState::NoContact => Ok(()),
        }
    }

    fn compute_tangential_moduli_with_coupling(
        &self,
        ctx: &ResolutionContext<'_>,
        element: &ContactElement,
        stiffness: &mut [Real],
        scratch: &mut Scratch,
        coupling_nodes: &mut [Idx],
        coupling: &mut [Real],
    ) -> Result<usize> {
        if self.mu == 0.0 || ctx.compat.friction_moduli {
            self.compute_tangential_moduli(ctx, element, stiffness, scratch)?;
            return Ok(0);
        }
        stiffness.fill(0.0);
        self.consistent_friction_moduli(ctx, element, stiffness, Some((coupling_nodes, coupling)))
    }

    fn parse(&mut self, section: &akantu_parser::ParserSection, permissive: bool) -> Result<()> {
        self.parse_section(section, permissive)
    }
}

/// Constructor of a resolution type: `(spatial dimension, option, id)`.
pub type ResolutionConstructor = fn(usize, &str, &str) -> Result<Box<dyn Resolution>>;

/// Registry of resolution types (upstream `ResolutionFactory`, filled by
/// `INSTANTIATE_RESOLUTION`; built-ins: `penalty_linear`,
/// `penalty_quadratic`).
#[derive(Clone)]
pub struct ResolutionFactory {
    constructors: BTreeMap<String, ResolutionConstructor>,
}

impl fmt::Debug for ResolutionFactory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolutionFactory")
            .field("types", &self.constructors.keys().collect::<Vec<_>>())
            .finish()
    }
}

fn check_resolution_dim(dim: usize, name: &str) -> Result<()> {
    if (1..=3).contains(&dim) {
        Ok(())
    } else {
        Err(Error::invalid(format!(
            "The dimension {dim}is not a valid dimension for the contact resolution {name}"
        )))
    }
}

fn new_penalty_linear(dim: usize, _opt: &str, id: &str) -> Result<Box<dyn Resolution>> {
    check_resolution_dim(dim, "penalty_linear")?;
    Ok(Box::new(ResolutionPenalty::new(id, PenaltyLaw::Linear)))
}

fn new_penalty_quadratic(dim: usize, _opt: &str, id: &str) -> Result<Box<dyn Resolution>> {
    check_resolution_dim(dim, "penalty_quadratic")?;
    Ok(Box::new(ResolutionPenalty::new(id, PenaltyLaw::Quadratic)))
}

impl Default for ResolutionFactory {
    fn default() -> Self {
        let mut constructors: BTreeMap<String, ResolutionConstructor> = BTreeMap::new();
        constructors.insert("penalty_linear".into(), new_penalty_linear);
        constructors.insert("penalty_quadratic".into(), new_penalty_quadratic);
        Self { constructors }
    }
}

impl ResolutionFactory {
    /// Register a resolution type (upstream `registerAllocator`; an
    /// existing name is an error).
    pub fn register(&mut self, name: &str, constructor: ResolutionConstructor) -> Result<()> {
        if self.constructors.contains_key(name) {
            return Err(Error::AlreadyExists {
                kind: "contact resolution type",
                id: name.into(),
            });
        }
        self.constructors.insert(name.into(), constructor);
        Ok(())
    }

    /// Registered type names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.constructors.keys().map(String::as_str)
    }

    /// Instantiate a resolution (upstream `allocate(type, dim, opt, model,
    /// id)`; unknown type → error).
    pub fn allocate(
        &self,
        ty: &str,
        dim: usize,
        opt: &str,
        id: &str,
    ) -> Result<Box<dyn Resolution>> {
        let c = self
            .constructors
            .get(ty)
            .ok_or_else(|| Error::not_found("contact resolution type", ty))?;
        c(dim, opt, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::ElementType;

    #[test]
    fn brackets() {
        assert_eq!(macaulay(-1.0), 0.0);
        assert_eq!(macaulay(2.0), 2.0);
        assert_eq!(macaulay(-0.0).to_bits(), (-0.0f64).to_bits());
        assert!(macaulay(Real::NAN).is_nan());
        assert_eq!(heaviside(0.0), 1.0);
        assert_eq!(heaviside(-0.0), 1.0);
        assert_eq!(heaviside(-1e-300), 0.0);
    }

    #[test]
    fn shape_matrices() {
        let mut a = vec![0.0; 3 * MAX_ELEMENT_DOFS];
        let nd = shape_function_matrix(ElementType::Segment2, 2, &[0.5], &mut a).unwrap();
        assert_eq!(nd, 6);
        // row 0: [1, 0, -N1, 0, -N2, 0] with N = (0.25, 0.75)
        let r0: Vec<Real> = (0..6).map(|c| a[c * 2]).collect();
        let r1: Vec<Real> = (0..6).map(|c| 1.0 * a[1 + c * 2]).collect();
        assert_eq!(r0, vec![1.0, 0.0, -0.25, 0.0, -0.75, 0.0]);
        assert_eq!(r1, vec![0.0, 1.0, 0.0, -0.25, 0.0, -0.75]);
        // derivative matrices of a triangle: corrected rows vs raw chunks
        let mut d = vec![0.0; 2 * 3 * MAX_ELEMENT_DOFS];
        derivative_shape_function_matrices(ElementType::Triangle3, 3, &[0.2, 0.3], false, &mut d)
            .unwrap();
        let nd = 12;
        // A_0 column of node 1, x row = ∂N0/∂ξ = −1
        assert_eq!(d[3 * 3], -1.0);
        assert_eq!(d[6 * 3], 1.0); // ∂N1/∂ξ
        assert_eq!(d[9 * 3], 0.0); // ∂N2/∂ξ
        let a1 = &d[3 * nd..];
        assert_eq!(a1[3 * 3], -1.0); // ∂N0/∂η
        assert_eq!(a1[6 * 3], 0.0); // ∂N1/∂η
        assert_eq!(a1[9 * 3], 1.0); // ∂N2/∂η
        let mut r = vec![0.0; 2 * 3 * MAX_ELEMENT_DOFS];
        derivative_shape_function_matrices(ElementType::Triangle3, 3, &[0.2, 0.3], true, &mut r)
            .unwrap();
        // raw chunk 0 = [∂N0/∂ξ, ∂N0/∂η, ∂N1/∂ξ] = [−1, −1, 1]
        assert_eq!((r[3 * 3], r[6 * 3], r[9 * 3]), (-1.0, -1.0, 1.0));
        // 2-D: identical
        let mut x = vec![0.0; 2 * 3 * MAX_ELEMENT_DOFS];
        let mut y = vec![0.0; 2 * 3 * MAX_ELEMENT_DOFS];
        derivative_shape_function_matrices(ElementType::Segment2, 2, &[0.1], false, &mut x)
            .unwrap();
        derivative_shape_function_matrices(ElementType::Segment2, 2, &[0.1], true, &mut y).unwrap();
        assert_eq!(x, y);
    }

    #[test]
    fn penalty_parameters() {
        let mut r = ResolutionPenalty::new("m:0:penalty_linear", PenaltyLaw::Linear);
        assert_eq!(
            r.param_names(),
            vec![
                "name",
                "mu",
                "is_master_deformable",
                "epsilon_n",
                "epsilon_t"
            ]
        );
        let p = akantu_parser::Parser::parse_str(
            "contact_resolution penalty_linear [\n name = contact\n mu = 0.3\n epsilon_n = 4200e7\n epsilon_t = 1e3\n is_master_deformable = true\n]\n",
            "r.dat",
        )
        .unwrap();
        r.parse(&p.root.sub_sections[0], false).unwrap();
        assert_eq!(r.name, "contact");
        assert_eq!((r.mu, r.epsilon_n, r.epsilon_t), (0.3, 4200e7, 1e3));
        assert!(r.is_master_deformable);
        assert!(r.set_param("name", ParamValue::String("x".into())).is_err()); // not writable
        r.set_param("mu", ParamValue::Real(0.1)).unwrap();
        assert_eq!(r.get_param("mu").unwrap().as_real().unwrap(), 0.1);
        let bad = akantu_parser::Parser::parse_str(
            "contact_resolution penalty_linear [\n name = c\n epsilon = 1\n]\n",
            "r.dat",
        )
        .unwrap();
        assert!(r.parse(&bad.root.sub_sections[0], false).is_err());
        assert!(r.parse(&bad.root.sub_sections[0], true).is_ok());
        assert_eq!(r.normal_traction(-1.0), 0.0);
        assert_eq!(r.normal_traction(2.0), 4200e7 * 2.0);
        let q = ResolutionPenalty {
            law: PenaltyLaw::Quadratic,
            ..r.clone()
        };
        assert_eq!(q.normal_traction(2.0), 4200e7 * (4.0 + 2.0));
    }

    #[test]
    fn factory() {
        let f = ResolutionFactory::default();
        assert_eq!(
            f.names().collect::<Vec<_>>(),
            vec!["penalty_linear", "penalty_quadratic"]
        );
        let r = f
            .allocate("penalty_quadratic", 2, "", "m:0:penalty_quadratic")
            .unwrap();
        assert_eq!(r.resolution_type(), "penalty_quadratic");
        assert!(f.allocate("penalty", 2, "", "x").is_err());
        assert!(f.allocate("penalty_linear", 4, "", "x").is_err());
        let mut g = f.clone();
        assert!(g.register("penalty_linear", new_penalty_linear).is_err());
        g.register("my_penalty", new_penalty_linear).unwrap();
        assert!(g.allocate("my_penalty", 3, "", "x").is_ok());
    }
}
