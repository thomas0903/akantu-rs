// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_cohesive/materials/constitutive_laws/
// material_cohesive_linear{.hh,.cc,_inline_impl.hh},
// material_cohesive_bilinear.{hh,cc}.

//! `cohesive_linear` (CMAT-001: extrinsic/intrinsic linear softening,
//! Camacho–Ortiz effective opening) and `cohesive_bilinear` (CMAT-002).
//!
//! Per quadrature point (operand order of upstream's Eigen expressions,
//! fixed-size `dim` vectors: halving-tree reductions):
//! `δ = √(β²/κ² ‖Δt‖² + Δn²)` (`Δn` excluded on penetration, which adds a
//! penalty contact traction), `δ_max ← max(δ_max, δ)`, `d = min(δ_max/δc,
//! 1)`, `T = (β²/κ Δt + Δn)·σc/δ_max·(1 − d)`, `T = T_insertion` at `d = 0`.
//! With a `δ_max` history the default takes the maximum with the converged
//! value, `δ_max = max(δ_max,prev, δ)`, so an unconverged Newton iterate
//! does not damage the point (UD-241);
//! `CohesiveUpstreamCompat::delta_max_per_iterate` reproduces upstream,
//! which updates the current maximum at every traction evaluation.

use akantu_core::math::{DEFAULT_TOLERANCE, are_float_equal};
use akantu_core::{
    Element, ElementKind, ElementType, Error, GhostType, Idx, Real, Result, TypeFilter,
};
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT};
use akantu_model::InternalField;
use akantu_model::constitutive_law::LawContext;
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};
use akantu_solid::material::MaterialBase;

use super::InsertionInputs;
use crate::material::{
    CohesiveCore, MaterialCohesive, PAR_READ, block, block_and_previous_mut, block_mut,
    nb_quad_cohesive, nb_quad_facet,
};
use crate::vecops::{dot_fixed, dot_seq, norm_fixed, std_max, std_min};

/// `Math::getTolerance()` (default value).
const TOL: Real = DEFAULT_TOLERANCE;

/// Member state of upstream's linear law after one quadrature point
/// (`normal_opening_norm`, `tangential_opening(_norm)`, `penetration`),
/// read by derived laws (friction).
#[derive(Clone, Copy, Debug, Default)]
pub struct QuadState {
    /// Upstream `penetration`.
    pub penetration: bool,
    /// Upstream `normal_opening_norm` of the traction; the tangent passes
    /// the normal opening of its full opening (upstream's member holds the
    /// recomputed tangential value there, which no law reads).
    pub normal_opening_norm: Real,
    /// Upstream `tangential_opening_norm`.
    pub tangential_opening_norm: Real,
    /// Upstream `tangential_opening`.
    pub tangential_opening: [Real; 3],
}

/// Parameters and internals of `MaterialCohesiveLinear`.
#[derive(Clone, Debug)]
pub struct Linear {
    /// `beta`.
    pub beta: Real,
    /// `G_c`.
    pub g_c: Real,
    /// `penalty`.
    pub penalty: Real,
    /// `volume_s` (reference volume of the `σc` scaling, 0 = off).
    pub volume_s: Real,
    /// `m_s` (Weibull exponent of the scaling).
    pub m_s: Real,
    /// `kappa`.
    pub kappa: Real,
    /// `contact_after_breaking`.
    pub contact_after_breaking: bool,
    /// `max_quad_stress_insertion`.
    pub max_quad_stress_insertion: bool,
    /// `recompute` (unused by the law).
    pub recompute: bool,
    beta2_kappa2: Real,
    beta2_kappa: Real,
    beta2_inv: Real,
    /// Effective critical stress per cohesive quadrature point (random).
    pub sigma_c_eff: InternalField<Real>,
    /// Effective critical opening.
    pub delta_c_eff: InternalField<Real>,
    /// Traction at insertion.
    pub insertion_stress: InternalField<Real>,
    /// Upstream `Math::getTolerance()` (global; default `1e2·ε`).
    pub tol: Real,
}

/// Upstream `MaterialCohesiveLinear::computeEffectiveNorm(stress, normal,
/// tangent, normal_traction)`: `stress` the side-averaged facet stress
/// (run-time `dim × dim`, column-major, given as the two sides), writes the
/// normal traction and returns `√(⟨tₙ⟩₊² + ‖tₜ‖²/β²)` (`beta2_inv` = `1/β²`).
fn effective_norm(
    beta2_inv: Real,
    dim: usize,
    s1: &[Real],
    s2: &[Real],
    normal: &[Real],
    tangent: &[Real],
    normal_traction: &mut [Real],
) -> Real {
    // `stress * normal`: run-time rows/depth on the left → sequential
    for i in 0..dim {
        let coeff = |k: usize| (s1[i + dim * k] + s2[i + dim * k]) / 2. * normal[k];
        let mut r = coeff(0);
        for k in 1..dim {
            r += coeff(k);
        }
        normal_traction[i] = r;
    }
    let mut normal_contrib = dot_seq(normal_traction, normal, dim);
    let mut tangent_contrib = 0.0;
    if dim == 2 {
        let t = dot_seq(normal_traction, &tangent[0..2], dim);
        tangent_contrib += t * t;
    } else if dim == 3 {
        for v in 0..2 {
            let t = dot_seq(normal_traction, &tangent[3 * v..3 * v + 3], dim);
            tangent_contrib += t * t;
        }
    }
    tangent_contrib = tangent_contrib.sqrt();
    normal_contrib = std_max(0.0, normal_contrib);
    (normal_contrib * normal_contrib + tangent_contrib * tangent_contrib * beta2_inv).sqrt()
}

/// [`effective_norm`] for a compile-time `D` (2 or 3) with the two sides in
/// one slice `fs` (`2·D²` values): the same operations in the same order,
/// the slices bounded once (performance; bitwise equal, tested).
#[inline(always)]
fn effective_norm_fixed<const D: usize>(
    beta2_inv: Real,
    fs: &[Real],
    normal: &[Real],
    tangent: &[Real],
    normal_traction: &mut [Real],
) -> Real {
    let s1 = &fs[..D * D];
    let s2 = &fs[D * D..2 * D * D];
    let normal = &normal[..D];
    let nt = &mut normal_traction[..D];
    for i in 0..D {
        let coeff = |k: usize| (s1[i + D * k] + s2[i + D * k]) / 2. * normal[k];
        let mut r = coeff(0);
        for k in 1..D {
            r += coeff(k);
        }
        nt[i] = r;
    }
    let dot = |b: &[Real]| {
        let mut r = nt[0] * b[0];
        for k in 1..D {
            r += nt[k] * b[k];
        }
        r
    };
    let mut normal_contrib = dot(normal);
    let mut tangent_contrib = 0.0;
    if D == 2 {
        let t = dot(&tangent[..2]);
        tangent_contrib += t * t;
    } else if D == 3 {
        let tangent = &tangent[..6];
        for v in 0..2 {
            let t = dot(&tangent[3 * v..3 * v + 3]);
            tangent_contrib += t * t;
        }
    }
    tangent_contrib = tangent_contrib.sqrt();
    normal_contrib = std_max(0.0, normal_contrib);
    (normal_contrib * normal_contrib + tangent_contrib * tangent_contrib * beta2_inv).sqrt()
}

impl Linear {
    pub(crate) fn new(core: &mut CohesiveCore) -> Result<Self> {
        core.use_previous_delta_max = true;
        let zero = akantu_parser::param::RandomParameter {
            base_value: 0.0,
            distribution: None,
        };
        let mut l = Self {
            beta: 0.0,
            g_c: 0.0,
            penalty: 0.0,
            volume_s: 0.0,
            m_s: 1.0,
            kappa: 1.0,
            contact_after_breaking: false,
            max_quad_stress_insertion: false,
            recompute: false,
            beta2_kappa2: 0.0,
            beta2_kappa: 0.0,
            beta2_inv: 0.0,
            sigma_c_eff: InternalField::new_random("sigma_c_eff", 1, zero.to_core())?,
            delta_c_eff: InternalField::new("delta_c_eff", 1, 0.0)?,
            insertion_stress: InternalField::new("insertion_stress", core.dim, 0.0)?,
            tol: TOL,
        };
        l.update_internal_parameters();
        Ok(l)
    }

    /// `β²/κ²`.
    pub(crate) fn beta2_kappa2(&self) -> Real {
        self.beta2_kappa2
    }

    /// `β²/κ`.
    pub(crate) fn beta2_kappa(&self) -> Real {
        self.beta2_kappa
    }

    /// Upstream `updateInternalParameters`.
    pub(crate) fn update_internal_parameters(&mut self) {
        self.beta2_kappa2 = self.beta * self.beta / self.kappa / self.kappa;
        self.beta2_kappa = self.beta * self.beta / self.kappa;
        self.beta2_inv = if are_float_equal(self.beta, 0.0, self.tol) {
            0.0
        } else {
            1. / self.beta / self.beta
        };
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, &InternalField<Real>)> {
        vec![
            ("sigma_c_eff", &self.sigma_c_eff),
            ("delta_c_eff", &self.delta_c_eff),
            ("insertion_stress", &self.insertion_stress),
        ]
    }

    pub(crate) fn fields_mut(&mut self) -> Vec<(&'static str, &mut InternalField<Real>)> {
        vec![
            ("sigma_c_eff", &mut self.sigma_c_eff),
            ("delta_c_eff", &mut self.delta_c_eff),
            ("insertion_stress", &mut self.insertion_stress),
        ]
    }

    /// Upstream `MaterialCohesiveLinear::initMaterial` after
    /// `MaterialCohesive::initMaterial`: `delta_c_eff` default (`delta_c`, or
    /// `2 G_c / σc`) and, extrinsic, `scaleInsertionTraction`.
    pub(crate) fn after_init(
        &mut self,
        core: &mut CohesiveCore,
        ctx: &LawContext<'_>,
    ) -> Result<()> {
        if !are_float_equal(core.delta_c, 0.0, self.tol) {
            self.delta_c_eff.set_default_value(core.delta_c);
        } else {
            let sigma_c = core.sigma_c_value();
            self.delta_c_eff.set_default_value(2. * self.g_c / sigma_c);
        }
        if core.is_extrinsic {
            self.scale_insertion_traction(core, ctx)?;
        }
        Ok(())
    }

    /// Upstream `scaleInsertionTraction` (Zhou–Molinari 2004): `σc` of every
    /// facet scaled by `(volume_s / V)^(1/m_s)`, `V` the volume of its two
    /// neighbours (no-op when `volume_s = 0`).
    fn scale_insertion_traction(
        &self,
        core: &mut CohesiveCore,
        ctx: &LawContext<'_>,
    ) -> Result<()> {
        if are_float_equal(self.volume_s, 0.0, self.tol) {
            return Ok(());
        }
        let mesh = ctx.services.mesh();
        let fm = mesh
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("cohesive law without facet mesh".into()))?;
        let fe = ctx.services.fe_engine("")?;
        let e2s = fm.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
        let types: Vec<ElementType> = fm
            .element_types(TypeFilter {
                spatial_dimension: Some(core.dim - 1),
                ghost_type: GhostType::NotGhost,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        for ty in types {
            let nq = nb_quad_facet(ty)?;
            let mut sig = block_mut(&mut core.sigma_c, ty, GhostType::NotGhost)?;
            let nb = sig.len() / nq.max(1);
            for f in 0..nb {
                let facet = Element::try_new(ty, f, GhostType::NotGhost)?;
                let mut volume = 0.0;
                for elem in &e2s.element(facet)?[0] {
                    if elem.is_null() {
                        continue;
                    }
                    let nqe = fe.nb_integration_points(elem.ty(), elem.ghost_type())?;
                    let ones = vec![1.0; nqe];
                    volume += fe.integrate_scalar(
                        akantu_core::array::ArrayView::new(&ones, 1)?,
                        elem.ty(),
                        elem.ghost_type(),
                        Some(&[elem.index()]),
                    )?;
                }
                let factor = (self.volume_s / volume).powf(1. / self.m_s);
                for v in &mut sig.as_mut_slice()[f * nq..(f + 1) * nq] {
                    // (σ − 0)·factor + 0 (`base_sigma_c_v` is a zero vector)
                    *v = (*v - 0.0) * factor + 0.0;
                }
            }
        }
        Ok(())
    }

    /// Upstream `computeEffectiveNorm` ([`effective_norm`]) at one facet
    /// quadrature point, `fs` holding the two sides' stresses
    /// (`2·dim²` values): the fixed-size form for `dim` 2 and 3.
    #[inline]
    fn effective_norm_at(
        &self,
        dim: usize,
        fs: &[Real],
        normal: &[Real],
        tangent: &[Real],
        normal_traction: &mut [Real],
    ) -> Real {
        match dim {
            3 => effective_norm_fixed::<3>(self.beta2_inv, fs, normal, tangent, normal_traction),
            2 => effective_norm_fixed::<2>(self.beta2_inv, fs, normal, tangent, normal_traction),
            _ => effective_norm(
                self.beta2_inv,
                dim,
                &fs[..dim * dim],
                &fs[dim * dim..],
                normal,
                tangent,
                normal_traction,
            ),
        }
    }

    /// Upstream `MaterialCohesiveLinear::checkInsertion(check_only)`: for
    /// every checked facet of the law, the effective norm of the averaged
    /// facet stress at each quadrature point; above the facet's strength the
    /// facet is flagged for insertion and, unless `check_only`, the new
    /// cohesive quadrature points get `σc,eff = norm`, the normal traction
    /// as insertion stress and previous traction (negated in 1-D/2-D) and
    /// `δc,eff` (`2 G_c/σ` or `σ_limit/σ·δ_c`) — appended to the law's
    /// arrays before the elements exist. Returns the facets whose new
    /// elements' parameters were appended, in the order of the arrays
    /// (the elements must join this law in that order).
    pub(crate) fn check_insertion(
        &mut self,
        core: &mut CohesiveCore,
        inputs: &mut InsertionInputs<'_>,
        check_only: bool,
    ) -> Result<Vec<Element>> {
        let dim = core.dim;
        let mut prepared = Vec::new();
        let g = GhostType::NotGhost;
        let fm = inputs.mesh_facets;
        let types: Vec<ElementType> = fm
            .element_types(TypeFilter {
                spatial_dimension: Some(dim - 1),
                ghost_type: g,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        for type_facet in types {
            let type_cohesive =
                type_facet
                    .cohesive_type()
                    .ok_or(Error::UnsupportedElementType {
                        ty: type_facet,
                        context: "checkInsertion (no cohesive type)",
                    })?;
            let nq = nb_quad_facet(type_facet)?;
            if nb_quad_cohesive(type_cohesive)? != nq {
                return Err(Error::invalid(
                    "the cohesive element and the corresponding facet do not have the same \
                     numbers of integration points",
                ));
            }
            let facets_check = inputs.check_facets.get(type_facet, g)?.as_slice();
            let f_stress = inputs.facet_stress.get(type_facet, g)?.as_slice();
            let tangents = inputs.tangents.get(type_facet, g)?.as_slice();
            let normals = inputs
                .facet_fe
                .normals_on_integration_points(type_facet, g)?
                .as_slice();
            let ntc = dim * (dim - 1);
            let mut new_sigmas = Vec::new();
            let mut new_tractions: Vec<[Real; 3]> = Vec::new();
            let mut new_delta = Vec::new();
            {
                // borrowed for the scan (no per-step copies of the filter
                // and the strengths, no per-facet allocation)
                let filter: &[Idx] = match core.facet_filter.get(type_facet, g) {
                    Ok(a) => a.as_slice(),
                    Err(_) => &[],
                };
                let sigma_limits = block(&core.sigma_c, type_facet, g);
                if !core.compat.sigma_c_zipped_per_quad && sigma_limits.len() < filter.len() * nq {
                    // e.g. `add_facet` after initialisation:
                    // the facet strengths do not cover the facet filter
                    return Err(Error::DimensionMismatch {
                        context: "checkInsertion: sigma_c shorter than the facet filter \
                                  (facets added after initialisation?)",
                        expected: filter.len() * nq,
                        got: sigma_limits.len(),
                    });
                }
                let mut stress_check = vec![0.0; nq];
                let mut normal_traction = vec![[0.0; 3]; nq];
                let nb_pairs = if core.compat.sigma_c_zipped_per_quad {
                    filter.len().min(sigma_limits.len())
                } else {
                    filter.len()
                };
                for (k, &facet) in filter.iter().enumerate().take(nb_pairs) {
                    let facet = akantu_core::try_to_usize(facet)?;
                    if !facets_check.get(facet).copied().unwrap_or(false) {
                        continue;
                    }
                    for q in 0..nq {
                        let cq = facet * nq + q;
                        let fs = &f_stress[cq * 2 * dim * dim..(cq + 1) * 2 * dim * dim];
                        stress_check[q] = self.effective_norm_at(
                            dim,
                            fs,
                            &normals[cq * dim..(cq + 1) * dim],
                            &tangents[cq * ntc..(cq + 1) * ntc],
                            &mut normal_traction[q][..dim],
                        );
                    }
                    // `stress_check.mean()` (run-time size)
                    let mut final_stress = stress_check[0];
                    for v in &stress_check[1..] {
                        final_stress += v;
                    }
                    final_stress /= nq as Real;
                    if self.max_quad_stress_insertion {
                        // `*std::max_element` (first maximum)
                        let mut m = stress_check[0];
                        for &v in &stress_check[1..] {
                            if m < v {
                                m = v;
                            }
                        }
                        final_stress = m;
                    }
                    // UD-040: upstream pairs facet k with strength entry k
                    let zipped = core.compat.sigma_c_zipped_per_quad;
                    let limit_q = |q: usize| {
                        if zipped {
                            sigma_limits[k]
                        } else {
                            sigma_limits[k * nq + q]
                        }
                    };
                    let limit: Real = if zipped {
                        sigma_limits[k]
                    } else {
                        let lq = &sigma_limits[k * nq..(k + 1) * nq];
                        let mut m = lq[0];
                        for v in &lq[1..] {
                            m += v;
                        }
                        m / nq as Real
                    };
                    if final_stress > limit {
                        inputs
                            .insertion_facets
                            .get_mut(type_facet, g)?
                            .as_mut_slice()[facet] = true;
                        if check_only {
                            continue;
                        }
                        // Only the facets whose
                        // parameters are appended below (a
                        // textual merge puts this line after the
                        // `facets_check` test above, which is wrong)
                        prepared.push(Element::try_new(type_facet, facet, g)?);
                        for q in 0..nq {
                            let new_sigma = stress_check[q];
                            let mut nt = normal_traction[q];
                            if dim != 3 {
                                for x in nt.iter_mut().take(dim) {
                                    *x *= -1.;
                                }
                            }
                            new_sigmas.push(new_sigma);
                            new_tractions.push(nt);
                            let new_d = if are_float_equal(core.delta_c, 0.0, self.tol) {
                                2. * self.g_c / new_sigma
                            } else {
                                limit_q(q) / new_sigma * core.delta_c
                            };
                            new_delta.push(new_d);
                        }
                    }
                }
            }
            let mut sig = block_mut(&mut self.sigma_c_eff, type_cohesive, g)?;
            let old = sig.len();
            let n_new = new_sigmas.len();
            sig.try_resize(old + n_new, 0.0)?;
            sig.as_mut_slice()[old..].copy_from_slice(&new_sigmas);
            let mut ins = block_mut(&mut self.insertion_stress, type_cohesive, g)?;
            ins.try_resize(old + n_new, 0.0)?;
            let trac_old = core.tractions.previous_mut(type_cohesive, g)?;
            trac_old.try_resize(old + n_new, 0.0)?;
            let mut del = block_mut(&mut self.delta_c_eff, type_cohesive, g)?;
            del.try_resize(old + n_new, 0.0)?;
            del.as_mut_slice()[old..old + n_new].copy_from_slice(&new_delta);
            // the insertion-stress guard taken above is still alive (explicit
            // guard `Drop`): written through it
            for (q, nt) in new_tractions.iter().enumerate() {
                ins.as_mut_slice()[(old + q) * dim..(old + q + 1) * dim]
                    .copy_from_slice(&nt[..dim]);
            }
            let trac_old = core.tractions.previous_mut(type_cohesive, g)?;
            for (q, nt) in new_tractions.iter().enumerate() {
                trac_old.as_mut_slice()[(old + q) * dim..(old + q + 1) * dim]
                    .copy_from_slice(&nt[..dim]);
            }
        }
        Ok(prepared)
    }

    /// Lengths of the not-ghost cohesive blocks [`Linear::check_insertion`]
    /// appends to (`σc,eff`, `δc,eff`, insertion stress, previous
    /// tractions), per cohesive type — restored by
    /// [`Linear::restore_insertion_marks`] when the insertion transaction
    /// fails before commit.
    pub(crate) fn insertion_marks(&self, core: &CohesiveCore) -> Vec<(ElementType, [usize; 4])> {
        let g = GhostType::NotGhost;
        // the four blocks of a cohesive type are created together (element
        // filter resize, or `check_insertion`)
        self.sigma_c_eff
            .values()
            .element_types(TypeFilter {
                spatial_dimension: None,
                ghost_type: g,
                kind: None,
            })
            .map(|ty| {
                let len = |a: Option<usize>| a.unwrap_or(0);
                (
                    ty,
                    [
                        len(self.sigma_c_eff.get(ty, g).ok().map(|a| a.len())),
                        len(self.delta_c_eff.get(ty, g).ok().map(|a| a.len())),
                        len(self.insertion_stress.get(ty, g).ok().map(|a| a.len())),
                        len(core.tractions.previous(ty, g).ok().map(|a| a.len())),
                    ],
                )
            })
            .collect()
    }

    /// Truncate the blocks to the lengths of [`Linear::insertion_marks`]
    /// (blocks created after the marks are emptied).
    ///
    /// # Panics
    /// If an internal field is exported to a foreign runtime and no native
    /// section of it is active (protocol violation,
    /// `SharedArray::borrow_mut`); absent blocks are skipped.
    pub(crate) fn restore_insertion_marks(
        &mut self,
        core: &mut CohesiveCore,
        marks: &[(ElementType, [usize; 4])],
    ) {
        let g = GhostType::NotGhost;
        let mark = |ty: ElementType, k: usize| {
            marks
                .iter()
                .find(|(t, _)| *t == ty)
                .map_or(0, |(_, m)| m[k])
        };
        for ty in self
            .sigma_c_eff
            .values()
            .element_types(TypeFilter {
                spatial_dimension: None,
                ghost_type: g,
                kind: None,
            })
            .collect::<Vec<_>>()
        {
            // absent blocks are skipped; a protocol violation of an
            // exported block panics (`borrow_mut`) instead of being taken
            // for absence
            if let Some(a) = self.sigma_c_eff.values_mut().as_map_mut().get_mut(ty, g) {
                a.borrow_mut().truncate(mark(ty, 0));
            }
            if let Some(a) = self.delta_c_eff.values_mut().as_map_mut().get_mut(ty, g) {
                a.borrow_mut().truncate(mark(ty, 1));
            }
            if let Some(a) = self
                .insertion_stress
                .values_mut()
                .as_map_mut()
                .get_mut(ty, g)
            {
                a.borrow_mut().truncate(mark(ty, 2));
            }
            if let Ok(a) = core.tractions.previous_mut(ty, g) {
                a.truncate(mark(ty, 3));
            }
        }
    }

    /// Upstream `computeTractionOnQuad` for every point of `(ty, ghost)`.
    pub(crate) fn compute_traction(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.compute_traction_with(core, ty, ghost, Some(0.0), &mut |_, _| {})
    }

    /// [`Linear::compute_traction`] with `after(state, traction)` called
    /// after every point (derived laws' `computeTractionOnQuad`).
    ///
    /// **UD-241**: with `floor = Some(f)`, the `δ_max`
    /// history (UD-028 default) and without `compat.delta_max_per_iterate`,
    /// the maximal opening is the converged one, `δ_max = max(max(δ_max
    /// previous, f), δ)` (`f`: `δ₀` for `cohesive_bilinear`, whose maximal
    /// opening starts there; 0 otherwise), so an unconverged Newton iterate
    /// leaves no damage; otherwise upstream's update of the current `δ_max`
    /// at every evaluation (`None`: `cohesive_linear_friction`, which
    /// restarts from the previous `δ_max` itself in implicit runs).
    pub(crate) fn compute_traction_with(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        floor: Option<Real>,
        after: &mut dyn FnMut(&QuadState, &mut [Real]),
    ) -> Result<()> {
        let converged_history = floor.is_some() && uses_converged_history(core);
        self.compute_traction_history(
            core,
            ty,
            ghost,
            floor.unwrap_or(0.0),
            converged_history,
            after,
        )
    }

    /// [`Linear::compute_traction_with`] with the history policy given by
    /// the caller: `converged_history` (requires the `δ_max` history):
    /// `δ_max = max(max(δ_max previous, floor), δ)` and the contact branch
    /// decided from the converged damage; otherwise upstream's
    /// per-evaluation update of the current `δ_max` and its contact
    /// decision from the current `damage`. `cohesive_linear_friction` uses
    /// it directly (**UD-247**: its implicit traction reads the converged
    /// history independently of the UD-241 switch, which does not apply to
    /// it).
    pub(crate) fn compute_traction_history(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        floor: Real,
        converged_history: bool,
        after: &mut dyn FnMut(&QuadState, &mut [Real]),
    ) -> Result<()> {
        let dim = core.dim;
        let CohesiveCore {
            opening,
            tractions,
            contact_tractions,
            contact_opening,
            delta_max,
            damage,
            normals,
            ..
        } = core;
        let n = block(normals, ty, ghost);
        let nq = n.len() / dim;
        let mut op_g = block_mut(opening, ty, ghost)?;
        let op = op_g.as_mut_slice();
        let mut tr_g = block_mut(tractions, ty, ghost)?;
        let tr = tr_g.as_mut_slice();
        let mut ct_g = block_mut(contact_tractions, ty, ghost)?;
        let ct = ct_g.as_mut_slice();
        let mut co_g = block_mut(contact_opening, ty, ghost)?;
        let co = co_g.as_mut_slice();
        let (mut dm_g, dm_prev) = block_and_previous_mut(
            delta_max,
            ty,
            ghost,
            nq,
            converged_history,
            "cohesive law: previous delta max shorter than the normals (UD-241)",
        )?;
        let dm = dm_g.as_mut_slice();
        let mut da_g = block_mut(damage, ty, ghost)?;
        let da = da_g.as_mut_slice();
        let sc = block(&self.sigma_c_eff, ty, ghost);
        let dc = block(&self.delta_c_eff, ty, ghost);
        let is = block(&self.insertion_stress, ty, ghost);
        for (len, what) in [
            (op.len() / dim, "opening"),
            (tr.len() / dim, "tractions"),
            (ct.len() / dim, "contact_tractions"),
            (co.len() / dim, "contact_opening"),
            (dm.len(), "delta max"),
            (da.len(), "damage"),
            (sc.len(), "sigma_c_eff"),
            (dc.len(), "delta_c_eff"),
            (is.len() / dim, "insertion_stress"),
        ] {
            if len < nq {
                return Err(Error::DimensionMismatch {
                    context: if what.is_empty() {
                        "cohesive_linear"
                    } else {
                        "cohesive_linear: internal shorter than the normals"
                    },
                    expected: nq,
                    got: len,
                });
            }
        }
        for q in 0..nq {
            let r = q * dim..(q + 1) * dim;
            let prev = dm_prev.map(|p| floored(p[q], floor));
            // With the converged history the contact
            // branch is decided from the converged damage, not from the
            // damage the previous trial left
            let contact_damage = prev.map_or(da[q], |m| committed_damage(m, dc[q], floor));
            let st = self.traction_on_quad(
                dim,
                &n[r.clone()],
                &mut op[r.clone()],
                &mut tr[r.clone()],
                &mut ct[r.clone()],
                &mut co[r.clone()],
                &mut dm[q],
                prev,
                contact_damage,
                &mut da[q],
                sc[q],
                dc[q],
                &is[r.clone()],
            );
            after(&st, &mut tr[r]);
        }
        Ok(())
    }

    /// Upstream `computeTractionOnQuad(args)`; `delta_max_prev`: the
    /// converged maximal opening (UD-241 default), `None`: upstream's
    /// per-evaluation update of `delta_max`; `contact_damage`: the damage
    /// that disables contact when it is 1 (without
    /// `contact_after_breaking`) — the converged damage with the converged
    /// history, upstream's current `damage` otherwise.
    // the arguments are upstream's per-point `getArguments` tuple (one slice
    // per internal); bundling them would copy per quadrature point
    #[allow(clippy::too_many_arguments)]
    fn traction_on_quad(
        &self,
        dim: usize,
        normal: &[Real],
        opening: &mut [Real],
        traction: &mut [Real],
        contact_traction: &mut [Real],
        contact_opening: &mut [Real],
        delta_max: &mut Real,
        delta_max_prev: Option<Real>,
        contact_damage: Real,
        damage: &mut Real,
        sigma_c: Real,
        delta_c: Real,
        insertion_stress: &[Real],
    ) -> QuadState {
        let norm = dot_fixed(opening, normal, dim);
        let mut normal_opening = [0.0; 3];
        let mut tangential_opening = [0.0; 3];
        for i in 0..dim {
            normal_opening[i] = normal[i] * norm;
            tangential_opening[i] = opening[i] - normal_opening[i];
        }
        let tn = norm_fixed(&tangential_opening, dim);
        let mut delta = tn * tn * self.beta2_kappa2;
        let mut penetration = Penetration::Relative.test(norm, delta_c, self.tol);
        if !self.contact_after_breaking && are_float_equal(contact_damage, 1., self.tol) {
            penetration = false;
        }
        if penetration {
            for i in 0..dim {
                contact_traction[i] = normal_opening[i] * self.penalty;
                contact_opening[i] = normal_opening[i];
                opening[i] = tangential_opening[i];
                normal_opening[i] = 0.0;
            }
        } else {
            delta += norm * norm;
            contact_traction[..dim].fill(0.0);
            contact_opening[..dim].fill(0.0);
        }
        delta = delta.sqrt();
        *delta_max = std_max(delta_max_prev.unwrap_or(*delta_max), delta);
        *damage = std_min(*delta_max / delta_c, 1.);
        if are_float_equal(*damage, 1., self.tol) {
            traction[..dim].fill(0.0);
        } else if are_float_equal(*damage, 0., self.tol) {
            if penetration {
                traction[..dim].fill(0.0);
            } else {
                traction[..dim].copy_from_slice(&insertion_stress[..dim]);
            }
        } else {
            for i in 0..dim {
                traction[i] = (tangential_opening[i] * self.beta2_kappa + normal_opening[i])
                    * sigma_c
                    / *delta_max
                    * (1. - *damage);
            }
        }
        QuadState {
            penetration,
            normal_opening_norm: norm,
            tangential_opening_norm: tn,
            tangential_opening,
        }
    }

    /// Upstream `computeTangentTraction(type, tangent, ghost)`: `tangent`
    /// holds one column-major `dim × dim` matrix per point (zeroed by the
    /// caller); the `opening` internal is modified as upstream (contact
    /// opening added back, tangential part only on penetration).
    pub(crate) fn compute_tangent_traction(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
    ) -> Result<()> {
        self.compute_tangent_traction_with(
            core,
            ty,
            ghost,
            tangent,
            Some(0.0),
            Penetration::Relative,
            &mut |_, _| {},
        )
    }

    /// [`Linear::compute_tangent_traction`] with `after(state, tangent)`
    /// called after every point. `floor`: see
    /// [`Linear::compute_traction_with`] (UD-241: with the converged history
    /// the branch is chosen with `δ_max = max(max(δ_max previous, floor),
    /// δ)`, the maximal opening the traction uses, and contact is disabled
    /// by the converged damage); `None` for a law whose
    /// traction keeps upstream's per-evaluation `δ_max` (fatigue, friction):
    /// the current `δ_max` and `damage`, as upstream. `penetration`: the
    /// predicate of the law's traction, used by the tangent too (**UD-242**;
    /// `compat.tangent_penetration_sign`: upstream's `Δn < 0`).
    // `penetration` completes upstream's argument list (see
    // `traction_on_quad`)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute_tangent_traction_with(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
        floor: Option<Real>,
        penetration: Penetration,
        after: &mut dyn FnMut(&QuadState, &mut [Real]),
    ) -> Result<()> {
        let converged_history = floor.is_some() && uses_converged_history(core);
        self.compute_tangent_traction_history(
            core,
            ty,
            ghost,
            tangent,
            floor.unwrap_or(0.0),
            converged_history,
            penetration,
            after,
        )
    }

    /// [`Linear::compute_tangent_traction_with`] with the history policy
    /// given by the caller (see [`Linear::compute_traction_history`]). The
    /// state passed to `after` carries the normal opening of the full
    /// opening (`opening + contact_opening`, before the penetration branch
    /// drops it), as the traction's state does.
    // `penetration` completes upstream's argument list (see
    // `traction_on_quad`)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute_tangent_traction_history(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
        floor: Real,
        converged_history: bool,
        penetration: Penetration,
        after: &mut dyn FnMut(&QuadState, &mut [Real]),
    ) -> Result<()> {
        let dim = core.dim;
        let base_sigma_c = core.sigma_c_value();
        // UD-220: upstream uses the base `σc` (compat); the traction uses
        // the per-point `σc,eff`, so the consistent tangent does too
        let use_base = core.compat.tangent_base_sigma_c;
        let predicate = if core.compat.tangent_penetration_sign {
            Penetration::Sign
        } else {
            penetration
        };
        let contact_normal_stiffness = core.compat.tangent_contact_normal_stiffness;
        let failed_secant = core.compat.tangent_failed_secant;
        let CohesiveCore {
            opening,
            contact_opening,
            delta_max,
            damage,
            normals,
            ..
        } = core;
        let n = block(normals, ty, ghost);
        let nq = n.len() / dim;
        let mut op_g = block_mut(opening, ty, ghost)?;
        let op = op_g.as_mut_slice();
        let co = block(contact_opening, ty, ghost);
        let dm = block(delta_max, ty, ghost);
        let dm_prev = if converged_history {
            Some(previous_block(delta_max, ty, ghost, nq)?)
        } else {
            None
        };
        let da = block(damage, ty, ghost);
        let dc = block(&self.delta_c_eff, ty, ghost);
        let sc = block(&self.sigma_c_eff, ty, ghost);
        if op.len() < nq * dim
            || co.len() < nq * dim
            || dm.len() < nq
            || da.len() < nq
            || dc.len() < nq
            || (!use_base && sc.len() < nq)
            || tangent.len() < nq * dim * dim
        {
            return Err(Error::DimensionMismatch {
                context: "cohesive_linear tangent: internal shorter than the normals",
                expected: nq,
                got: dm.len().min(da.len()).min(dc.len()),
            });
        }
        for q in 0..nq {
            let r = q * dim..(q + 1) * dim;
            let sigma_c = if use_base { base_sigma_c } else { sc[q] };
            let prev = dm_prev.map(|p| floored(p[q], floor));
            let contact_damage = prev.map_or(da[q], |m| committed_damage(m, dc[q], floor));
            let st = self.tangent_on_quad(
                dim,
                &mut tangent[q * dim * dim..(q + 1) * dim * dim],
                &n[r.clone()],
                &mut op[r.clone()],
                &co[r],
                dm[q],
                prev,
                contact_damage,
                dc[q],
                sigma_c,
                predicate,
                contact_normal_stiffness,
                failed_secant,
            );
            after(&st, &mut tangent[q * dim * dim..(q + 1) * dim * dim]);
        }
        Ok(())
    }

    /// Upstream `computeTangentTractionOnQuad(tangent, args)`;
    /// `delta_max_prev`: the converged maximal opening (UD-241 default: the
    /// branch uses `max(delta_max_prev, δ)`, `δ` before the fictitious
    /// opening of a closed point), `None`: the current `delta_max`;
    /// `contact_damage`: see [`Linear::traction_on_quad`]; `predicate`: the
    /// penetration test (**UD-242**); `contact_normal_stiffness`: upstream's
    /// cohesive normal stiffness on the penetration branch (**UD-243**
    /// compat), else the derivative of the traction `β²/κ (I − n⊗n)·t/δ`;
    /// `failed_secant`: upstream's unloading secant once `δ_max > δc`
    /// (**UD-1160** compat), else the derivative of the zero traction of a
    /// failed point.
    // the arguments are upstream's per-point `getArguments` tuple (one slice
    // per internal); bundling them would copy per quadrature point
    #[allow(clippy::too_many_arguments)]
    fn tangent_on_quad(
        &self,
        dim: usize,
        tangent: &mut [Real],
        normal: &[Real],
        opening: &mut [Real],
        contact_opening: &[Real],
        delta_max: Real,
        delta_max_prev: Option<Real>,
        contact_damage: Real,
        delta_c: Real,
        sigma_c: Real,
        predicate: Penetration,
        contact_normal_stiffness: bool,
        failed_secant: bool,
    ) -> QuadState {
        for i in 0..dim {
            opening[i] += contact_opening[i];
        }
        let mut nn_norm = dot_fixed(opening, normal, dim);
        // the normal opening of the full opening, for the derived laws (the
        // penetration branch below recomputes `nn_norm` from the
        // tangential part, as upstream)
        let full_normal_opening = nn_norm;
        let mut normal_opening = [0.0; 3];
        let mut tangential_opening = [0.0; 3];
        for i in 0..dim {
            normal_opening[i] = normal[i] * nn_norm;
            tangential_opening[i] = opening[i] - normal_opening[i];
        }
        let tn = norm_fixed(&tangential_opening, dim);
        let mut delta = tn * tn * self.beta2_kappa2;
        let mut penetration = predicate.test(nn_norm, delta_c, self.tol);
        if !self.contact_after_breaking && are_float_equal(contact_damage, 1., self.tol) {
            penetration = false;
        }
        let mut derivative = 0.0;
        let mut t = 0.0;
        let nout = |i: usize, j: usize| normal[i] * normal[j];
        if penetration {
            for j in 0..dim {
                for i in 0..dim {
                    tangent[i + dim * j] = self.penalty * (tangent[i + dim * j] + nout(i, j));
                }
            }
            opening[..dim].copy_from_slice(&tangential_opening[..dim]);
            nn_norm = dot_fixed(opening, normal, dim);
            for i in 0..dim {
                normal_opening[i] = normal[i] * nn_norm;
            }
        } else {
            delta += nn_norm * nn_norm;
        }
        delta = delta.sqrt();
        // UD-241: the maximal opening of the traction at this opening
        let delta_max = delta_max_prev.map_or(delta_max, |p| std_max(p, delta));
        if delta < self.tol {
            delta = delta_c / 1000.;
        }
        if delta >= delta_max {
            if delta <= delta_c {
                derivative = -sigma_c / (delta * delta);
                t = sigma_c * (1. - delta / delta_c);
            } else {
                derivative = 0.;
                t = 0.;
            }
        } else if delta < delta_max {
            // UD-1160: once `δ_max ≥ δc` the traction is zero (damage 1), so
            // is its derivative; upstream's secant (compat) turns negative
            // past δc. `std_max(x, 0)` returns `x` unchanged while `x ≥ 0`.
            let mut remaining = 1. - delta_max / delta_c;
            if !failed_secant {
                remaining = std_max(remaining, 0.);
            }
            let tmax = sigma_c * remaining;
            t = tmax / delta_max * delta;
        }
        let mut t_tilde = [0.0; 3];
        let mut t_hat = [0.0; 3];
        for i in 0..dim {
            let mm = opening[i] * self.beta2_kappa2;
            t_tilde[i] = normal_opening[i] * (1. - self.beta2_kappa2) + mm;
            t_hat[i] = normal_opening[i] + self.beta2_kappa * tangential_opening[i];
        }
        // UD-243: on penetration the traction has no cohesive normal part
        let normal_secant = !penetration || contact_normal_stiffness;
        // prov = t_hat t_tildeᵀ · derivative / δ + nn; tangent += provᵀ
        for j in 0..dim {
            for i in 0..dim {
                let id = if i == j { self.beta2_kappa } else { 0.0 };
                let nn = if normal_secant {
                    (nout(i, j) * (1. - self.beta2_kappa) + id) * t / delta
                } else {
                    (id - nout(i, j) * self.beta2_kappa) * t / delta
                };
                let prov = t_hat[i] * t_tilde[j] * derivative / delta + nn;
                tangent[j + dim * i] += prov;
            }
        }
        QuadState {
            penetration,
            normal_opening_norm: full_normal_opening,
            tangential_opening_norm: tn,
            tangential_opening,
        }
    }
}

/// `cohesive_bilinear`: `delta_0` on top of [`Linear`].
#[derive(Clone, Debug)]
pub struct Bilinear {
    /// The linear part.
    pub linear: Linear,
    /// `delta_0` (elastic limit opening).
    pub delta_0: Real,
}

impl Bilinear {
    /// Upstream `MaterialCohesiveBilinear::initMaterial` before the linear
    /// part: `σc,eff` draws from `σc`'s distribution.
    pub(crate) fn before_init(&mut self, core: &CohesiveCore) {
        if let Some(p) = core.sigma_c.random_parameter() {
            self.linear.sigma_c_eff.set_random_distribution(p.clone());
        }
    }

    /// Upstream `initMaterial` after the linear part: `δ_max` default
    /// `δ_0`, insertion stress 0 (both reset).
    pub(crate) fn after_init(
        &mut self,
        core: &mut CohesiveCore,
        ctx: &LawContext<'_>,
    ) -> Result<()> {
        self.linear.after_init(core, ctx)?;
        core.delta_max.set_default_value(self.delta_0);
        self.linear.insertion_stress.set_default_value(0.0);
        core.delta_max.reset();
        self.linear.insertion_stress.reset();
        Ok(())
    }

    /// Upstream `onElementsAdded`: for every new not-ghost cohesive element
    /// of the law, optional `σc` scaling, then `δc = 2 G_c/σc` and `σc ←
    /// σc·δc/(δc − δ0)`.
    ///
    /// **UD-218.** Upstream visits every element of the event (also those
    /// of other laws) and indexes its arrays with the element's **mesh**
    /// index (`compat.bilinear_mesh_indices`; an index past the law's arrays
    /// is refused — upstream reads out of bounds). By default only the
    /// law's own new elements are processed, at their law-local index: the
    /// handler appends them, in event order, to the end of the element
    /// filter before this hook runs, so they are among the last `k` filter
    /// entries of their type (`k` = the event's elements of that type).
    pub(crate) fn on_elements_added(
        &mut self,
        core: &mut CohesiveCore,
        base: &MaterialBase,
        ctx: &LawContext<'_>,
        elements: &[Element],
    ) -> Result<()> {
        let scale = !are_float_equal(self.linear.volume_s, 0.0, self.linear.tol);
        let upstream = core.compat.bilinear_mesh_indices;
        let is_new = |el: &Element| {
            el.ghost_type() == GhostType::NotGhost && el.kind() == ElementKind::Cohesive
        };
        // law-local index of the law's new elements (default)
        let mut local: std::collections::BTreeMap<
            ElementType,
            std::collections::HashMap<Idx, usize>,
        > = std::collections::BTreeMap::new();
        if !upstream {
            let mut counts: std::collections::BTreeMap<ElementType, usize> =
                std::collections::BTreeMap::new();
            for el in elements.iter().filter(|e| is_new(e)) {
                *counts.entry(el.ty()).or_default() += 1;
            }
            for (ty, k) in counts {
                let filter = base
                    .element_filter
                    .get(ty, GhostType::NotGhost)
                    .map(|a| a.as_slice())
                    .unwrap_or(&[]);
                let start = filter.len().saturating_sub(k);
                local.insert(
                    ty,
                    filter[start..]
                        .iter()
                        .enumerate()
                        .map(|(j, &e)| (e, start + j))
                        .collect(),
                );
            }
        }
        for el in elements.iter().filter(|e| is_new(e)) {
            let ty = el.ty();
            let nq = nb_quad_cohesive(ty)?;
            let idx = if upstream {
                el.idx()
            } else {
                match local.get(&ty).and_then(|m| m.get(&el.index())) {
                    Some(&l) => l,
                    // an element of another law
                    None => continue,
                }
            };
            {
                let sig = block_mut(&mut self.linear.sigma_c_eff, ty, GhostType::NotGhost)?;
                let len = sig.len();
                let del_len = block(&self.linear.delta_c_eff, ty, GhostType::NotGhost).len();
                if len < (idx + 1) * nq || del_len < (idx + 1) * nq {
                    return Err(Error::OutOfRange {
                        what: if upstream {
                            "cohesive_bilinear: new element (mesh index used as law index, \
                             upstream compat UD-218)"
                        } else {
                            "cohesive_bilinear: new element (law-local index)"
                        },
                        index: idx,
                        len: len / nq.max(1),
                    });
                }
            }
            if scale {
                self.scale_traction(ctx, *el, idx, nq)?;
            }
            let g_c = self.linear.g_c;
            let delta_0 = self.delta_0;
            let mut sig = block_mut(&mut self.linear.sigma_c_eff, ty, GhostType::NotGhost)?;
            let mut del = block_mut(&mut self.linear.delta_c_eff, ty, GhostType::NotGhost)?;
            for q in 0..nq {
                let k = idx * nq + q;
                let d = 2. * g_c / sig.as_slice()[k];
                del.as_mut_slice()[k] = d;
                if d - delta_0 < self.linear.tol {
                    return Err(Error::invalid(format!(
                        "delta_0 = {delta_0} must be lower than delta_c = {d}, modify your \
                         material file"
                    )));
                }
                sig.as_mut_slice()[k] *= d / (d - delta_0);
            }
        }
        Ok(())
    }

    /// Upstream `scaleTraction(el, sigma_c_vec)`: the volume of the bulk
    /// neighbours of the mesh element `el`, the strengths of law-local
    /// element `idx` scaled.
    fn scale_traction(
        &mut self,
        ctx: &LawContext<'_>,
        el: Element,
        idx: usize,
        nq: usize,
    ) -> Result<()> {
        let base_sigma_c = self.linear.sigma_c_eff.parameter_value();
        let mesh = ctx.services.mesh();
        let fm = mesh
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("cohesive law without facet mesh".into()))?;
        let fe = ctx.services.fe_engine("")?;
        let sides = fm
            .data()
            .element_map(SUBELEMENT_TO_ELEMENT)?
            .element(el)?
            .to_vec();
        let e2s = fm.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
        let mut volume = 0.0;
        for facet in sides.iter().take(2) {
            for elem in &e2s.element(*facet)?[0] {
                if elem.is_null() || elem.kind() == ElementKind::Cohesive {
                    continue;
                }
                let nqe = fe.nb_integration_points(elem.ty(), elem.ghost_type())?;
                let ones = vec![1.0; nqe];
                volume += fe.integrate_scalar(
                    akantu_core::array::ArrayView::new(&ones, 1)?,
                    elem.ty(),
                    elem.ghost_type(),
                    Some(&[elem.index()]),
                )?;
            }
        }
        let factor = (self.linear.volume_s / volume).powf(1. / self.linear.m_s);
        let mut sig = block_mut(&mut self.linear.sigma_c_eff, el.ty(), GhostType::NotGhost)?;
        for v in &mut sig.as_mut_slice()[idx * nq..(idx + 1) * nq] {
            *v = (*v - base_sigma_c) * factor + base_sigma_c;
        }
        Ok(())
    }

    /// Upstream `computeTraction`: the linear law, then `d = clamp((δ_max −
    /// δ0)/(δc − δ0), 0, 1)`.
    pub(crate) fn compute_traction(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        // UD-241: the converged maximal opening starts at δ₀
        self.linear
            .compute_traction_with(core, ty, ghost, Some(self.delta_0), &mut |_, _| {})?;
        let dm = block(&core.delta_max, ty, ghost).to_vec();
        let dc = block(&self.linear.delta_c_eff, ty, ghost);
        let mut da_g = block_mut(&mut core.damage, ty, ghost)?;
        let da = da_g.as_mut_slice();
        for ((d, m), c) in da.iter_mut().zip(&dm).zip(&dc) {
            *d = std_max((m - self.delta_0) / (c - self.delta_0), 0.);
            *d = std_min(*d, 1.);
        }
        Ok(())
    }
}

/// **UD-241**: the linear family uses the converged maximal opening (the
/// `δ_max` history exists — UD-028 default — and
/// `compat.delta_max_per_iterate` is off).
fn uses_converged_history(core: &CohesiveCore) -> bool {
    core.delta_max.has_history() && !core.compat.delta_max_per_iterate
}

/// The damage a traction evaluation stores for the maximal opening
/// `delta_max` (`cohesive_bilinear`: `floor` = δ₀, `clamp((δ_max − δ₀)/(δc −
/// δ₀), 0, 1)`; otherwise `min(δ_max/δc, 1)`), in the operation order of the
/// traction, so the converged damage of an explicit step is bitwise the
/// damage the step stored.
pub(crate) fn committed_damage(delta_max: Real, delta_c: Real, floor: Real) -> Real {
    let d = if floor > 0.0 {
        std_max((delta_max - floor) / (delta_c - floor), 0.)
    } else {
        delta_max / delta_c
    };
    std_min(d, 1.)
}

/// Penetration test of a linear-family traction (**UD-242**: the tangent
/// uses its traction's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Penetration {
    /// `Δn/δc < −tol` (`cohesive_linear` and the laws using its traction).
    Relative,
    /// `Δn < −tol` (`cohesive_linear_fatigue`).
    Absolute,
    /// `Δn < 0` (upstream's linear tangent, `compat.tangent_penetration_sign`).
    Sign,
}

impl Penetration {
    pub(crate) fn test(self, normal_opening: Real, delta_c: Real, tol: Real) -> bool {
        match self {
            Self::Relative => normal_opening / delta_c < -tol,
            Self::Absolute => normal_opening < -tol,
            Self::Sign => normal_opening < 0.0,
        }
    }
}

/// `max(previous, floor)` (the floor only when positive: `cohesive_linear`
/// reads the previous value unchanged).
pub(crate) fn floored(previous: Real, floor: Real) -> Real {
    if floor > 0.0 {
        std_max(previous, floor)
    } else {
        previous
    }
}

/// The previous values of `(ty, ghost)` of a field with history, at least
/// `nq` of them.
fn previous_block(
    f: &InternalField<Real>,
    ty: ElementType,
    ghost: GhostType,
    nq: usize,
) -> Result<&[Real]> {
    let p = match f.previous(ty, ghost) {
        Ok(a) => a.as_slice(),
        Err(_) if nq == 0 => &[],
        Err(e) => return Err(e),
    };
    if p.len() < nq {
        return Err(Error::DimensionMismatch {
            context: "cohesive law: previous values shorter than the normals (UD-241)",
            expected: nq,
            got: p.len(),
        });
    }
    Ok(p)
}

fn lin(s: &MaterialCohesive) -> Result<&Linear> {
    s.law
        .linear()
        .ok_or_else(|| Error::invalid("not a linear-family cohesive law"))
}

fn lin_mut(s: &mut MaterialCohesive) -> Result<&mut Linear> {
    s.law
        .linear_mut()
        .ok_or_else(|| Error::invalid("not a linear-family cohesive law"))
}

macro_rules! real_param {
    ($name:literal, $field:ident, $desc:literal) => {
        ParamDescriptor::new(
            $name,
            ParamKind::Real,
            PAR_READ,
            $desc,
            |s| ParamValue::Real(lin(s).map_or(0.0, |l| l.$field)),
            Some(|s, v| {
                lin_mut(s)?.$field = v.as_real()?;
                Ok(())
            }),
        )
    };
}

macro_rules! bool_param {
    ($name:literal, $field:ident, $access:expr, $desc:literal) => {
        ParamDescriptor::new(
            $name,
            ParamKind::Bool,
            $access,
            $desc,
            |s| ParamValue::Bool(lin(s).is_ok_and(|l| l.$field)),
            Some(|s, v| {
                lin_mut(s)?.$field = v.as_bool()?;
                Ok(())
            }),
        )
    };
}

pub(crate) const P_BETA: ParamDescriptor<MaterialCohesive> =
    real_param!("beta", beta, "Beta parameter");
pub(crate) const P_GC: ParamDescriptor<MaterialCohesive> =
    real_param!("G_c", g_c, "Mode I fracture energy");
pub(crate) const P_PENALTY: ParamDescriptor<MaterialCohesive> =
    real_param!("penalty", penalty, "Penalty coefficient");
pub(crate) const P_VOLUME_S: ParamDescriptor<MaterialCohesive> =
    real_param!("volume_s", volume_s, "Reference volume for sigma_c scaling");
pub(crate) const P_M_S: ParamDescriptor<MaterialCohesive> =
    real_param!("m_s", m_s, "Weibull exponent for sigma_c scaling");
pub(crate) const P_KAPPA: ParamDescriptor<MaterialCohesive> =
    real_param!("kappa", kappa, "Kappa parameter");
pub(crate) const P_CONTACT_AFTER_BREAKING: ParamDescriptor<MaterialCohesive> = bool_param!(
    "contact_after_breaking",
    contact_after_breaking,
    PAR_READ,
    "Activation of contact when the elements are fully damaged"
);
pub(crate) const P_MAX_QUAD: ParamDescriptor<MaterialCohesive> = bool_param!(
    "max_quad_stress_insertion",
    max_quad_stress_insertion,
    PAR_READ,
    "Insertion of cohesive element when stress is high enough just on one quadrature point"
);
pub(crate) const P_RECOMPUTE: ParamDescriptor<MaterialCohesive> = bool_param!(
    "recompute",
    recompute,
    ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::MODIFIABLE.0),
    "recompute solution"
);

pub(crate) const P_DELTA_0: ParamDescriptor<MaterialCohesive> = ParamDescriptor::new(
    "delta_0",
    ParamKind::Real,
    PAR_READ,
    "Elastic limit displacement",
    |s| match &s.law {
        super::CohesiveLaw::Bilinear(b) => ParamValue::Real(b.delta_0),
        _ => ParamValue::Real(0.0),
    },
    Some(|s, v| match &mut s.law {
        super::CohesiveLaw::Bilinear(b) => {
            b.delta_0 = v.as_real()?;
            Ok(())
        }
        _ => Err(Error::invalid("delta_0 is a cohesive_bilinear parameter")),
    }),
);

#[cfg(test)]
mod tests {
    use super::*;

    const TY: ElementType = ElementType::Cohesive2d4;
    const G: GhostType = GhostType::NotGhost;

    /// Test values (xorshift64*): special values (signed zeros, infinities,
    /// default quiet NaNs, subnormals, extremes), arbitrary bit patterns and
    /// finite values over a wide exponent range.
    fn draw(state: &mut u64) -> Real {
        const SPECIAL: [u64; 12] = [
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x7ff0_0000_0000_0000,
            0xfff0_0000_0000_0000,
            0x7ff8_0000_0000_0000,
            0xfff8_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x800f_ffff_ffff_ffff,
            0x0010_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
            0x3ff0_0000_0000_0000,
            0xbff0_0000_0000_0000,
        ];
        let mut next = || {
            let mut x = *state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            *state = x;
            x.wrapping_mul(0x2545_f491_4f6c_dd1d)
        };
        let r = next();
        let x = match r % 8 {
            0 => Real::from_bits(SPECIAL[(r >> 8) as usize % SPECIAL.len()]),
            1 => Real::from_bits(next()),
            _ => {
                let m = (next() >> 11) as Real / (1u64 << 53) as Real;
                let e = ((r >> 8) % 41) as i32 - 20;
                let s = if (r >> 40) & 1 == 1 { -1.0 } else { 1.0 };
                s * m * Real::powi(2.0, e)
            }
        };
        if x.is_nan() {
            Real::from_bits(0x7ff8_0000_0000_0000 | (x.to_bits() & (1 << 63)))
        } else {
            x
        }
    }

    /// Identical bits, or two default quiet NaNs differing only in the sign.
    fn same(a: Real, b: Real) -> bool {
        let (x, y) = (a.to_bits(), b.to_bits());
        let nan = 0x7ff8_0000_0000_0000;
        x == y || (x & !(1 << 63) == nan && y & !(1 << 63) == nan)
    }

    /// The fixed-size effective norm is bitwise the run-time one (value
    /// and normal traction), special values included.
    #[test]
    fn effective_norm_fixed_is_bitwise_the_general_one() {
        fn check<const D: usize>(state: &mut u64) {
            for _ in 0..20_000 {
                let fs: Vec<Real> = (0..2 * D * D).map(|_| draw(state)).collect();
                let n: Vec<Real> = (0..D).map(|_| draw(state)).collect();
                let t: Vec<Real> = (0..D * (D - 1)).map(|_| draw(state)).collect();
                let b = draw(state).abs();
                let (mut nt1, mut nt2) = ([7.0; 3], [7.0; 3]);
                let v1 = effective_norm(b, D, &fs[..D * D], &fs[D * D..], &n, &t, &mut nt1);
                let v2 = effective_norm_fixed::<D>(b, &fs, &n, &t, &mut nt2);
                assert!(same(v1, v2), "D = {D}: {v1:e} vs {v2:e}");
                for (x, y) in nt1.iter().zip(&nt2) {
                    assert!(same(*x, *y), "D = {D}: traction {x:e} vs {y:e}");
                }
            }
        }
        let mut state = 0x0bad_5eed_1234_5678_u64;
        check::<2>(&mut state);
        check::<3>(&mut state);
    }

    fn set(f: &mut InternalField<Real>, v: &[Real]) {
        let mut a = block_mut(f, TY, G).unwrap();
        a.try_resize(v.len() / a.nb_component(), 0.0).unwrap();
        a.as_mut_slice().copy_from_slice(v);
    }

    /// One quadrature point of a 2-D linear law with `σc = 1` and
    /// `σc,eff = 2`, `β = 0.8`, `κ = 1.5` (unsymmetric tangent).
    fn state(compat: bool) -> (CohesiveCore, Linear) {
        let mut core = CohesiveCore::new(2).unwrap();
        core.sigma_c.set_random_distribution(
            akantu_parser::param::RandomParameter {
                base_value: 1.0,
                distribution: None,
            }
            .to_core(),
        );
        core.compat.tangent_base_sigma_c = compat;
        let mut l = Linear::new(&mut core).unwrap();
        l.beta = 0.8;
        l.kappa = 1.5;
        l.update_internal_parameters();
        set(&mut core.normals, &[0., 1.]);
        set(&mut core.tractions, &[0., 0.]);
        set(&mut core.contact_tractions, &[0., 0.]);
        set(&mut core.contact_opening, &[0., 0.]);
        set(&mut core.damage, &[0.]);
        set(&mut l.sigma_c_eff, &[2.0]);
        set(&mut l.delta_c_eff, &[0.5]);
        set(&mut l.insertion_stress, &[0., 0.]);
        (core, l)
    }

    /// Traction at `opening` on the loading branch (`δ_max` reset).
    fn traction(core: &mut CohesiveCore, l: &Linear, opening: [Real; 2]) -> [Real; 2] {
        set(&mut core.opening, &opening);
        set(&mut core.delta_max, &[0.]);
        l.compute_traction(core, TY, G).unwrap();
        let t = block(&core.tractions, TY, G);
        [t[0], t[1]]
    }

    fn tangent(compat: bool, opening: [Real; 2]) -> ([Real; 4], [[Real; 2]; 2]) {
        let (mut core, l) = state(compat);
        let h = 1e-7;
        let mut fd = [[0.0; 2]; 2];
        for j in 0..2 {
            let mut p = opening;
            let mut m = opening;
            p[j] += h;
            m[j] -= h;
            let (tp, tm) = (traction(&mut core, &l, p), traction(&mut core, &l, m));
            for i in 0..2 {
                fd[i][j] = (tp[i] - tm[i]) / (2. * h);
            }
        }
        // the state the tangent is evaluated at (after the traction update)
        let _ = traction(&mut core, &l, opening);
        let mut k = [0.0; 4];
        l.compute_tangent_traction(&mut core, TY, G, &mut k)
            .unwrap();
        (k, fd)
    }

    /// UD-220: the tangent is the derivative of the
    /// traction, which uses the per-point `σc,eff`. Upstream (compat, the
    /// behaviour before the fix) uses the base `σc` and is off by `σc/σc,eff`.
    #[test]
    fn tangent_is_the_derivative_of_the_traction_with_sigma_c_eff() {
        let opening = [0.05, 0.1];
        let (k, fd) = tangent(false, opening);
        for i in 0..2 {
            for j in 0..2 {
                // tangent = provᵀ, prov(i, j) = ∂T_i/∂Δ_j
                let kij = k[j + 2 * i];
                assert!(
                    (kij - fd[i][j]).abs() <= 1e-6 * fd[i][j].abs().max(1.),
                    "∂T{i}/∂Δ{j}: tangent {kij:e} vs finite difference {:e}",
                    fd[i][j]
                );
            }
        }
        assert!(
            (k[1] - k[2]).abs() > 1e-3,
            "κ ≠ 1 must give an unsymmetric tangent"
        );
        let (kc, _) = tangent(true, opening);
        for i in 0..2 {
            for j in 0..2 {
                let kij = kc[j + 2 * i];
                assert!(
                    (2. * kij - fd[i][j]).abs() <= 1e-6 * fd[i][j].abs().max(1.),
                    "compat: ∂T{i}/∂Δ{j} = {:e}, tangent {kij:e} (expected half)",
                    fd[i][j]
                );
            }
        }
    }

    /// A point with the `δ_max` history (UD-028 default) whose converged
    /// maximal opening is `prev` (saved), and the UD-241 switch.
    fn history_state(per_iterate: bool, prev: Real) -> (CohesiveCore, Linear) {
        let (mut core, l) = state(false);
        core.compat.delta_max_per_iterate = per_iterate;
        set(&mut core.delta_max, &[prev]);
        core.delta_max.initialize_history();
        core.delta_max.save_current_values().unwrap();
        (core, l)
    }

    /// UD-241: an unconverged Newton iterate that
    /// overshoots (δ = 0.3) leaves no damage at the converged opening
    /// (δ = 0.2) by default: `δ_max = max(δ_max previous, δ)`; upstream
    /// (per iterate) keeps the iterate's 0.3 and unloads.
    #[test]
    fn ud241_unconverged_iterate_leaves_no_damage() {
        // σc,eff = 2, δc,eff = 0.5, normal (0, 1): mode I, δ = Δn
        for per_iterate in [false, true] {
            let (mut core, l) = history_state(per_iterate, 0.1);
            let _ = traction(&mut core, &l, [0.0, 0.3]); // the iterate
            set(&mut core.opening, &[0.0, 0.2]); // the converged opening
            l.compute_traction(&mut core, TY, G).unwrap();
            let dm = block(&core.delta_max, TY, G)[0];
            let d = block(&core.damage, TY, G)[0];
            let t = block(&core.tractions, TY, G)[1];
            // the `traction` helper resets the current δ_max to 0 before the
            // iterate (it does not matter by default); upstream's iterate
            // raises it to 0.3 either way
            let (dm_e, d_e) = if per_iterate { (0.3, 0.6) } else { (0.2, 0.4) };
            assert_eq!(dm, dm_e, "per_iterate {per_iterate}");
            assert!((d - d_e).abs() <= 1e-15, "per_iterate {per_iterate}: {d}");
            // T = Δn σc,eff/δ_max (1 − d): loading 2(1 − 0.4) = 1.2; upstream
            // unloads on the secant of 0.3: 0.2·2/0.3·0.4
            let t_e = 0.2 * 2.0 / dm_e * (1. - d_e);
            assert!(
                (t - t_e).abs() <= 1e-15,
                "per_iterate {per_iterate}: {t} vs {t_e}"
            );
        }
    }

    /// UD-241: with the converged history the tangent is
    /// the derivative of `T(max(δ_max previous, δ))`, on the loading branch
    /// (δ = 0.305 > 0.25) and on the unloading secant (δ = 0.104 < 0.25),
    /// whatever the current `δ_max` holds (here an iterate's 0.4).
    #[test]
    fn ud241_tangent_is_the_derivative_of_the_converged_history_traction() {
        for opening in [[0.1, 0.3], [0.05, 0.1]] {
            let (mut core, l) = history_state(false, 0.25);
            let h = 1e-7;
            let mut fd = [[0.0; 2]; 2];
            for j in 0..2 {
                let mut p = opening;
                let mut m = opening;
                p[j] += h;
                m[j] -= h;
                let (tp, tm) = (traction(&mut core, &l, p), traction(&mut core, &l, m));
                for i in 0..2 {
                    fd[i][j] = (tp[i] - tm[i]) / (2. * h);
                }
            }
            let _ = traction(&mut core, &l, opening);
            // an overshooting iterate's value in the current array: ignored
            set(&mut core.delta_max, &[0.4]);
            let mut k = [0.0; 4];
            l.compute_tangent_traction(&mut core, TY, G, &mut k)
                .unwrap();
            for i in 0..2 {
                for j in 0..2 {
                    let kij = k[j + 2 * i];
                    assert!(
                        (kij - fd[i][j]).abs() <= 1e-6 * fd[i][j].abs().max(1.),
                        "opening {opening:?}: ∂T{i}/∂Δ{j}: tangent {kij:e} vs finite difference {:e}",
                        fd[i][j]
                    );
                }
            }
        }
    }

    /// UD-241: the converged maximal opening of
    /// `cohesive_bilinear` starts at δ₀ even where the history was never
    /// saved (its default is 0, upstream's copy of the field before δ₀ is
    /// set): the elastic branch, identical to upstream's evaluation.
    #[test]
    fn ud241_bilinear_maximal_opening_starts_at_delta_0() {
        let mut out = Vec::new();
        for per_iterate in [false, true] {
            let (mut core, l) = state(false);
            core.compat.delta_max_per_iterate = per_iterate;
            // the current δ_max holds δ₀ (bilinear default); the history 0
            set(&mut core.delta_max, &[0.05]);
            core.delta_max.initialize_history();
            assert_eq!(core.delta_max.previous(TY, G).unwrap().as_slice(), &[0.0]);
            let b = Bilinear {
                linear: l,
                delta_0: 0.05,
            };
            set(&mut core.opening, &[0.0, 0.02]);
            b.compute_traction(&mut core, TY, G).unwrap();
            assert_eq!(block(&core.delta_max, TY, G), &[0.05]);
            assert_eq!(block(&core.damage, TY, G), &[0.0]);
            out.push(block(&core.tractions, TY, G).to_vec());
        }
        // elastic: Δn σc,eff/δ₀ (1 − δ₀/δc,eff) = 0.02·2/0.05·0.9
        assert!((out[0][1] - 0.02 * 2. / 0.05 * 0.9).abs() <= 1e-15);
        assert_eq!(out[0], out[1]);
    }

    /// Cohesive plus contact traction of a trial at `opening` (the current
    /// `δ_max` and `damage` are whatever the previous trial left).
    fn total(core: &mut CohesiveCore, l: &Linear, opening: [Real; 2]) -> [Real; 2] {
        set(&mut core.opening, &opening);
        l.compute_traction(core, TY, G).unwrap();
        let t = block(&core.tractions, TY, G);
        let c = block(&core.contact_tractions, TY, G);
        [t[0] + c[0], t[1] + c[1]]
    }

    /// Central finite differences of [`total`] at `opening`, every
    /// evaluation preceded by the trial `before`, and the tangent at
    /// `opening` (after its traction): `(tangent, fd)` with `tangent[j + 2i]
    /// = ∂T_i/∂Δ_j`.
    fn tangent_and_fd(
        core: &mut CohesiveCore,
        l: &Linear,
        opening: [Real; 2],
        before: [Real; 2],
        h: Real,
    ) -> ([Real; 4], [[Real; 2]; 2]) {
        let mut fd = [[0.0; 2]; 2];
        for j in 0..2 {
            let mut p = opening;
            let mut m = opening;
            p[j] += h;
            m[j] -= h;
            let _ = total(core, l, before);
            let tp = total(core, l, p);
            let _ = total(core, l, before);
            let tm = total(core, l, m);
            for i in 0..2 {
                fd[i][j] = (tp[i] - tm[i]) / (2. * h);
            }
        }
        let _ = total(core, l, before);
        let _ = total(core, l, opening);
        let mut k = [0.0; 4];
        l.compute_tangent_traction(core, TY, G, &mut k).unwrap();
        (k, fd)
    }

    fn assert_tangent_matches(what: &str, k: &[Real; 4], fd: &[[Real; 2]; 2]) {
        for i in 0..2 {
            for j in 0..2 {
                let kij = k[j + 2 * i];
                assert!(
                    (kij - fd[i][j]).abs() <= 1e-6 * fd[i][j].abs().max(1.),
                    "{what}: ∂T{i}/∂Δ{j}: tangent {kij:e} vs finite difference {:e}",
                    fd[i][j]
                );
            }
        }
    }

    /// After a trial past complete failure (Δn = 0.6 >
    /// δc,eff = 0.5, damage 1) a compression trial with sliding (Δ = (0.03,
    /// −0.02)) of a point whose converged state is intact (δ_max previous
    /// 0.1) takes the contact branch — with both `contact_after_breaking`
    /// settings — and the tangent there is the derivative of the trial
    /// traction (every finite-difference evaluation preceded by the
    /// overshooting trial). Before the fix the traction read the trial's
    /// damage 1 (no contact); upstream's tangent adds the cohesive normal
    /// secant `t/δ` on the penetration branch (UD-243, compat control).
    #[test]
    fn co3_01_contact_branch_after_an_overshooting_trial() {
        let opening = [0.03, -0.02];
        let overshoot = [0.0, 0.6];
        for cab in [false, true] {
            let (mut core, mut l) = history_state(false, 0.1);
            l.penalty = 50.;
            l.contact_after_breaking = cab;
            let _ = total(&mut core, &l, overshoot);
            assert_eq!(block(&core.damage, TY, G), &[1.0], "cab {cab}");
            let t = total(&mut core, &l, opening);
            let c = block(&core.contact_tractions, TY, G)[1];
            assert_eq!(c, -0.02 * 50., "cab {cab}: penalty contact");
            // unloading secant of the converged state on the sliding:
            // β²/κ Δt σc,eff/δ_max (1 − d), d = 0.2
            let b2k = 0.8 * 0.8 / 1.5;
            assert!(
                (t[0] - b2k * 0.03 * 2. / 0.1 * 0.8).abs() <= 1e-14,
                "cab {cab}: {t:?}"
            );
            let (k, fd) = tangent_and_fd(&mut core, &l, opening, overshoot, 1e-7);
            assert_tangent_matches(&format!("cab {cab}"), &k, &fd);
            // UD-243 compat: upstream adds n⊗n·t/δ = t_max/δ_max = 16
            core.compat.tangent_contact_normal_stiffness = true;
            let (kc, _) = tangent_and_fd(&mut core, &l, opening, overshoot, 1e-7);
            assert!(
                (kc[3] - fd[1][1] - 2. * 0.8 / 0.1).abs() <= 1e-6 * 16.,
                "cab {cab}: compat ∂Ty/∂Δy {} vs {}",
                kc[3],
                fd[1][1]
            );
        }
    }

    /// With the converged history every trial is a
    /// function of the converged state and its own opening — the same trial
    /// after different earlier trials (through complete failure and
    /// compression) gives bitwise the same traction, contact and damage.
    #[test]
    fn co3_01_repeated_trials_are_bitwise_equal() {
        let trials = [
            [0.0, 0.6],
            [0.03, -0.02],
            [0.0, 0.3],
            [0.03, -0.02],
            [0.2, 0.0],
        ];
        for cab in [false, true] {
            let (mut core, mut l) = history_state(false, 0.1);
            l.penalty = 50.;
            l.contact_after_breaking = cab;
            let state = |core: &CohesiveCore| -> Vec<u64> {
                [
                    block(&core.tractions, TY, G),
                    block(&core.contact_tractions, TY, G),
                    block(&core.damage, TY, G),
                    block(&core.delta_max, TY, G),
                ]
                .iter()
                .flat_map(|a| a.iter().map(|v| v.to_bits()))
                .collect()
            };
            let mut first = Vec::new();
            for t in trials {
                let _ = total(&mut core, &l, t);
                first.push(state(&core));
            }
            for (k, t) in trials.iter().enumerate().rev() {
                let _ = total(&mut core, &l, *t);
                assert_eq!(state(&core), first[k], "cab {cab}: trial {t:?}");
            }
        }
    }

    /// `cohesive_bilinear` decides the contact from the
    /// converged damage `clamp((δ_max − δ₀)/(δc − δ₀))` — the damage its
    /// traction stores (bitwise, so explicit steps are unchanged) — and its
    /// tangent on the contact branch after an overshooting trial is the
    /// derivative of the trial traction.
    #[test]
    fn co3_01_bilinear_contact_branch_after_an_overshooting_trial() {
        let (mut core, mut l) = history_state(false, 0.1);
        l.penalty = 50.;
        let b = Bilinear {
            linear: l,
            delta_0: 0.05,
        };
        let eval = |core: &mut CohesiveCore, o: [Real; 2]| -> [Real; 2] {
            set(&mut core.opening, &o);
            b.compute_traction(core, TY, G).unwrap();
            let t = block(&core.tractions, TY, G);
            let c = block(&core.contact_tractions, TY, G);
            [t[0] + c[0], t[1] + c[1]]
        };
        // the stored damage of an evaluation equals the converged damage
        // of its maximal opening
        let _ = eval(&mut core, [0.0, 0.2]);
        let stored = block(&core.damage, TY, G)[0];
        assert_eq!(stored.to_bits(), committed_damage(0.2, 0.5, 0.05).to_bits());
        let (opening, overshoot) = ([0.03, -0.02], [0.0, 0.6]);
        let h = 1e-7;
        let mut fd = [[0.0; 2]; 2];
        for j in 0..2 {
            let (mut p, mut m) = (opening, opening);
            p[j] += h;
            m[j] -= h;
            let _ = eval(&mut core, overshoot);
            let tp = eval(&mut core, p);
            let _ = eval(&mut core, overshoot);
            let tm = eval(&mut core, m);
            for i in 0..2 {
                fd[i][j] = (tp[i] - tm[i]) / (2. * h);
            }
        }
        let _ = eval(&mut core, overshoot);
        let t = eval(&mut core, opening);
        assert_eq!(block(&core.contact_tractions, TY, G)[1], -0.02 * 50.);
        assert!(t[0] != 0.0, "unloading secant on the sliding");
        let mut k = [0.0; 4];
        b.linear
            .compute_tangent_traction_with(
                &mut core,
                TY,
                G,
                &mut k,
                Some(0.05),
                Penetration::Relative,
                &mut |_, _| {},
            )
            .unwrap();
        assert_tangent_matches("bilinear", &k, &fd);
    }

    /// UD-242: strictly inside the closure band `−tol·δc
    /// < Δn < 0` the traction applies no contact, so the default tangent
    /// (same predicate) is its derivative; upstream's tangent (`Δn < 0`,
    /// compat) adds the penalty stiffness. A coarse `tol` (1e-3) makes the
    /// band (|Δn| < 5e-4) wide enough for a finite-difference stencil (h =
    /// 1e-6 around Δn = −2.5e-4).
    #[test]
    fn ud242_tangent_strictly_inside_the_closure_band() {
        let (mut core, mut l) = history_state(false, 0.25);
        l.penalty = 50.;
        l.tol = 1e-3;
        let opening = [0.1, -2.5e-4];
        let (k, fd) = tangent_and_fd(&mut core, &l, opening, opening, 1e-6);
        assert_eq!(block(&core.contact_tractions, TY, G), &[0.0, 0.0]);
        assert_tangent_matches("default", &k, &fd);
        core.compat.tangent_penetration_sign = true;
        let (kc, _) = tangent_and_fd(&mut core, &l, opening, opening, 1e-6);
        assert!(
            kc[3] - fd[1][1] > 40.,
            "compat: upstream's tangent adds the penalty: {} vs {}",
            kc[3],
            fd[1][1]
        );
    }

    /// UD-242: the fatigue traction's penetration test is `Δn < −tol`
    /// (absolute), the linear one `Δn/δc < −tol`; the tangent predicate
    /// follows the given traction predicate (and `Δn < 0` in compat).
    #[test]
    fn ud242_penetration_predicates() {
        let tol = 1e-3;
        let dc = 0.5;
        // Δn = −8e-4: Δn/δc = −1.6e-3 < −tol (relative), Δn > −tol (absolute)
        assert!(Penetration::Relative.test(-8e-4, dc, tol));
        assert!(!Penetration::Absolute.test(-8e-4, dc, tol));
        assert!(Penetration::Sign.test(-1e-12, dc, tol));
        assert!(!Penetration::Relative.test(-1e-12, dc, tol));
    }

    // ------------------------------------------------------------------
    // UD-1160 (D-099 (1)): the tangent of a failed
    // point
    // ------------------------------------------------------------------

    use super::super::CohesiveLaw;

    /// Cohesive plus contact traction of `law` at `opening` (a trial
    /// against the committed state).
    fn law_total(core: &mut CohesiveCore, law: &mut CohesiveLaw, opening: [Real; 2]) -> [Real; 2] {
        set(&mut core.opening, &opening);
        law.compute_traction(core, TY, G).unwrap();
        let t = block(&core.tractions, TY, G);
        let c = block(&core.contact_tractions, TY, G);
        [t[0] + c[0], t[1] + c[1]]
    }

    /// `(K, J)` of `law` at `opening`: the tangent after the traction at
    /// `opening` (`K[j + 2i] = ∂T_i/∂Δ_j`) and central differences (step
    /// `h`).
    fn law_tangent_and_fd(
        core: &mut CohesiveCore,
        law: &mut CohesiveLaw,
        opening: [Real; 2],
        h: Real,
    ) -> ([Real; 4], [[Real; 2]; 2]) {
        let mut fd = [[0.0; 2]; 2];
        for j in 0..2 {
            let (mut p, mut m) = (opening, opening);
            p[j] += h;
            m[j] -= h;
            let tp = law_total(core, law, p);
            let tm = law_total(core, law, m);
            for i in 0..2 {
                fd[i][j] = (tp[i] - tm[i]) / (2. * h);
            }
        }
        let _ = law_total(core, law, opening);
        let mut k = [0.0; 4];
        law.compute_tangent_traction(core, TY, G, &mut k).unwrap();
        (k, fd)
    }

    /// Worst `|K − J|` relative to the largest `|J|` or `|K|` (0 when both
    /// vanish).
    fn rel_error(k: &[Real; 4], fd: &[[Real; 2]; 2]) -> Real {
        let mut scale: Real = 0.0;
        let mut worst: Real = 0.0;
        for i in 0..2 {
            for j in 0..2 {
                scale = scale.max(fd[i][j].abs()).max(k[j + 2 * i].abs());
                worst = worst.max((k[j + 2 * i] - fd[i][j]).abs());
            }
        }
        if scale == 0.0 { 0.0 } else { worst / scale }
    }

    /// `cohesive_linear` or `cohesive_bilinear` (δ₀ = 0.05) on the point
    /// of [`history_state`] (σc,eff 2, δc,eff 0.5, β 0.8, κ 1.5, normal
    /// (0, 1)) with penalty 50 and `contact_after_breaking = cab`.
    fn failed_law(bilinear: bool, committed: Real, cab: bool) -> (CohesiveCore, CohesiveLaw) {
        let (core, mut l) = history_state(false, committed);
        l.penalty = 50.;
        l.contact_after_breaking = cab;
        let law = if bilinear {
            CohesiveLaw::Bilinear(Bilinear {
                linear: l,
                delta_0: 0.05,
            })
        } else {
            CohesiveLaw::Linear(l)
        };
        (core, law)
    }

    /// **UD-1160** (D-099 (1)): once a point has failed
    /// past δc (committed `δ_max` = 0.8 > δc,eff = 0.5, damage 1) its
    /// cohesive traction is zero at every trial below `δ_max`: open below δc
    /// (δ = 0.21) and between δc and `δ_max` (δ = 0.62), compressed without
    /// `contact_after_breaking` (no contact: the normal opening counts in
    /// δ) and with it (the penalty alone). By default the tangent of
    /// `cohesive_linear` and `cohesive_bilinear` is the derivative of that
    /// traction — exactly zero when open, exactly the penalty `50 n⊗n` in
    /// contact — and the central differences agree (the law-level rule,
    /// 1e-6 of max(|J|, 1)). Upstream's secant (compat) adds `σc (1 −
    /// δ_max/δc)/δ_max = −1.5` times `β²/κ I + (1 − β²/κ) n⊗n` (open) or
    /// `β²/κ (I − n⊗n)` (contact): relative errors 1 and 1.3e-2.
    #[test]
    fn ud1160_failed_point_tangent_is_the_derivative_of_the_zero_traction() {
        // (label, opening, contact_after_breaking, default tangent)
        let cases: [(&str, [Real; 2], bool, [Real; 4]); 4] = [
            ("open, below delta_c", [0.1, 0.2], false, [0.0; 4]),
            (
                "open, between delta_c and delta_max",
                [0.3, 0.6],
                false,
                [0.0; 4],
            ),
            (
                "compressed, no contact after breaking",
                [0.03, -0.02],
                false,
                [0.0; 4],
            ),
            (
                "contact after breaking",
                [0.03, -0.02],
                true,
                [0.0, 0.0, 0.0, 50.0],
            ),
        ];
        for bilinear in [false, true] {
            for (label, opening, cab, expected) in cases {
                let what = format!("{} {label}", if bilinear { "bilinear" } else { "linear" });
                let (mut core, mut law) = failed_law(bilinear, 0.8, cab);
                let (k, fd) = law_tangent_and_fd(&mut core, &mut law, opening, 1e-7);
                let t = law_total(&mut core, &mut law, opening);
                assert_eq!(
                    block(&core.tractions, TY, G),
                    &[0.0, 0.0],
                    "{what}: failed point"
                );
                assert_eq!(block(&core.damage, TY, G), &[1.0], "{what}");
                assert_eq!(k, expected, "{what}: tangent (traction {t:?})");
                assert_tangent_matches(&what, &k, &fd);
                eprintln!("{what}: default relative error {:.2e}", rel_error(&k, &fd));
                // UD-1160 compat: upstream's negative secant
                core.compat.tangent_failed_secant = true;
                let (kc, fdc) = law_tangent_and_fd(&mut core, &mut law, opening, 1e-7);
                assert_eq!(
                    fdc, fd,
                    "{what}: the traction does not depend on the switch"
                );
                let err = rel_error(&kc, &fdc);
                eprintln!("{what}: upstream secant {kc:?}, relative error {err:.2e}");
                assert!(err > 1e-3, "{what}: upstream's secant must differ ({kc:?})");
                let b2k = 0.8 * 0.8 / 1.5;
                let secant = 2.0 * (1. - 0.8 / 0.5) / 0.8;
                assert!(
                    (kc[0] - k[0] - b2k * secant).abs() <= 1e-14,
                    "{what}: the tangential secant term ({} vs {})",
                    kc[0] - k[0],
                    b2k * secant
                );
            }
        }
    }

    /// **UD-1160**: while the point has not failed past δc (committed
    /// `δ_max` 0.3, and exactly δc = 0.5) the default tangent is upstream's
    /// secant bit for bit (`max(1 − δ_max/δc, 0) = 1 − δ_max/δc`), open and
    /// in contact.
    #[test]
    fn ud1160_tangent_unchanged_until_failure() {
        for bilinear in [false, true] {
            for committed in [0.3, 0.5] {
                for (opening, cab) in [([0.05, 0.1], false), ([0.03, -0.02], true)] {
                    let (mut core, mut law) = failed_law(bilinear, committed, cab);
                    let _ = law_total(&mut core, &mut law, opening);
                    let mut k = [0.0; 4];
                    law.compute_tangent_traction(&mut core, TY, G, &mut k)
                        .unwrap();
                    core.compat.tangent_failed_secant = true;
                    let _ = law_total(&mut core, &mut law, opening);
                    let mut kc = [0.0; 4];
                    law.compute_tangent_traction(&mut core, TY, G, &mut kc)
                        .unwrap();
                    assert_eq!(
                        k.map(Real::to_bits),
                        kc.map(Real::to_bits),
                        "bilinear {bilinear}, δ_max {committed}, {opening:?}"
                    );
                }
            }
        }
    }
}

/// Port of `test_material_cohesive_linear.cc` (TEST-119,
/// `TestMaterialCohesiveLinearFixture.{ModeI, ModeII, Cycles}` with
/// `test_material_cohesive_fixture.hh`): the per-point traction of
/// `MaterialCohesiveLinear` (`β = κ = 2`, `G_c = 10`, `σc = 1e6`, penalty
/// `1e11`, `δc = 2 G_c/σc`) along opening paths, against the analytic
/// traction and the trapezoidal dissipated energy, in 1-D, 2-D and 3-D.
/// Upstream draws the normal and the mode-II direction from a seeded
/// `std::mt19937`; fixed unit vectors are used here (same intent).
#[cfg(test)]
mod test_material_cohesive_linear {
    use super::*;

    struct Point {
        law: Linear,
        dim: usize,
        sigma_c: Real,
        delta_c: Real,
        insertion_stress: [Real; 3],
        delta_max: Real,
        damage: Real,
        contact_traction: [Real; 3],
        contact_opening: [Real; 3],
    }

    impl Point {
        fn new(dim: usize) -> Self {
            let mut core = CohesiveCore::new(dim).unwrap();
            let mut law = Linear::new(&mut core).unwrap();
            law.beta = 2.;
            law.kappa = 2.;
            law.g_c = 10.;
            law.penalty = 1e11;
            law.update_internal_parameters();
            let sigma_c = 1e6;
            Self {
                law,
                dim,
                sigma_c,
                delta_c: 2. * 10. / sigma_c,
                insertion_stress: [0.; 3],
                delta_max: 0.,
                damage: 0.,
                contact_traction: [0.; 3],
                contact_opening: [0.; 3],
            }
        }

        /// Upstream `computeTractions`: the traction of every opening
        /// (member state carried along), then `opening += contact_opening`,
        /// `traction += contact_traction`.
        fn tractions(&mut self, normal: &[Real], openings: &mut [Vec<Real>]) -> Vec<Vec<Real>> {
            let d = self.dim;
            openings
                .iter_mut()
                .map(|o| {
                    let mut t = vec![0.; d];
                    let _ = self.law.traction_on_quad(
                        d,
                        normal,
                        o,
                        &mut t,
                        &mut self.contact_traction[..d],
                        &mut self.contact_opening[..d],
                        &mut self.delta_max,
                        // every opening of the path is a converged state:
                        // upstream's update of the carried δ_max
                        None,
                        self.damage,
                        &mut self.damage,
                        self.sigma_c,
                        self.delta_c,
                        &self.insertion_stress[..d],
                    );
                    for i in 0..d {
                        o[i] += self.contact_opening[i];
                        t[i] += self.contact_traction[i];
                    }
                    t
                })
                .collect()
        }

        /// The fixture's analytic `traction(opening, normal)`.
        fn expected(&self, o: &[Real], n: &[Real]) -> Vec<Real> {
            let d = self.dim;
            let on: Real = (0..d).map(|i| o[i] * n[i]).sum();
            let normal_op: Vec<Real> = (0..d).map(|i| on * n[i]).collect();
            let tang: Vec<Real> = (0..d).map(|i| o[i] - normal_op[i]).collect();
            let nn: Real = normal_op.iter().map(|x| x * x).sum::<Real>();
            let tt: Real = tang.iter().map(|x| x * x).sum::<Real>();
            let (beta, kappa) = (2., 2.);
            let delta = (nn + tt * (beta / kappa) * (beta / kappa)).sqrt();
            if delta < 1e-16 {
                return self.insertion_stress[..d].to_vec();
            }
            let f = self.sigma_c * (self.delta_c - delta) / self.delta_c / delta;
            (0..d)
                .map(|i| f * (normal_op[i] + tang[i] * beta * beta / kappa))
                .collect()
        }
    }

    fn dot(a: &[Real], b: &[Real]) -> Real {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    /// `reset()` (one zero opening) then `addOpening(direction, start,
    /// stop, nb_steps)` segments.
    fn path(direction: &[Real], segments: &[(Real, Real)]) -> Vec<Vec<Real>> {
        let mut v = vec![vec![0.; direction.len()]];
        for &(start, stop) in segments {
            for s in 0..100 {
                let a = start + (stop - start) / 100. * (s + 1) as Real;
                v.push(direction.iter().map(|x| x * a).collect());
            }
        }
        v
    }

    /// The fixture's `dissipated()`: trapezoidal total work minus the last
    /// reversible energy.
    fn dissipated(openings: &[Vec<Real>], tractions: &[Vec<Real>]) -> Real {
        let d = openings[0].len();
        let (mut po, mut pt) = (vec![0.; d], vec![0.; d]);
        let (mut etot, mut erev) = (0., 0.);
        for (o, t) in openings.iter().zip(tractions) {
            let dop: Vec<Real> = (0..d).map(|i| o[i] - po[i]).collect();
            let st: Vec<Real> = (0..d).map(|i| t[i] + pt[i]).collect();
            etot += dot(&dop, &st) / 2.;
            erev = dot(t, o) / 2.;
            po.clone_from(o);
            pt.clone_from(t);
        }
        etot - erev
    }

    fn unit(v: &[Real]) -> Vec<Real> {
        let n = dot(v, v).sqrt();
        v.iter().map(|x| x / n).collect()
    }

    fn normal(dim: usize) -> Vec<Real> {
        unit(&[0.3, 0.8, 0.5][..dim])
    }

    /// Tangents as the fixture's `getRandomTangents` (2-D: `normal2d(n)`;
    /// 3-D: Gram–Schmidt of a vector, then `n × t0`).
    fn tangents(n: &[Real]) -> Vec<Vec<Real>> {
        match n.len() {
            2 => vec![unit(&[n[1], -n[0]])],
            3 => {
                let v = [0.9, 0.1, 0.4];
                let p = dot(&v, n);
                let t0 = unit(&[v[0] - p * n[0], v[1] - p * n[1], v[2] - p * n[2]]);
                let t1 = unit(&[
                    t0[1] * n[2] - t0[2] * n[1],
                    t0[2] * n[0] - t0[0] * n[2],
                    t0[0] * n[1] - t0[1] * n[0],
                ]);
                vec![t0, t1]
            }
            _ => vec![],
        }
    }

    #[test]
    fn mode_i() {
        for dim in 1..=3 {
            let mut p = Point::new(dim);
            let n = normal(dim);
            p.insertion_stress[..dim]
                .copy_from_slice(&n.iter().map(|x| x * p.sigma_c).collect::<Vec<_>>());
            let mut openings = path(&n, &[(0., p.delta_c)]);
            let tractions = p.tractions(&n, &mut openings);
            for (o, t) in openings.iter().zip(&tractions) {
                let tn = dot(t, &n);
                let tang: Real = (0..dim)
                    .map(|i| (t[i] - tn * n[i]).powi(2))
                    .sum::<Real>()
                    .sqrt();
                assert!(tang.abs() < 1e-9, "dim {dim}: tangential traction {tang:e}");
                let te = dot(&p.expected(o, &n), &n);
                assert!((te - tn).abs() < 1e-9, "dim {dim}: {tn} vs {te}");
            }
            let dis = dissipated(&openings, &tractions);
            assert!((10. - dis).abs() < 1e-6, "dim {dim}: dissipated {dis}");
        }
    }

    #[test]
    fn mode_ii() {
        for dim in 2..=3 {
            let mut p = Point::new(dim);
            let n = normal(dim);
            let t = tangents(&n);
            let (alpha, beta_mix) = (0.6, 0.7);
            let mut direction: Vec<Real> = t[0].iter().map(|x| alpha * x).collect();
            if dim > 2 {
                for (d, x) in direction.iter_mut().zip(&t[1]) {
                    *d += beta_mix * x;
                }
            }
            let direction = unit(&direction);
            let beta = p.law.beta;
            for (s, d) in p.insertion_stress.iter_mut().zip(&direction) {
                *s = beta * p.sigma_c * d;
            }
            let mut openings = path(&direction, &[(0., p.delta_c)]);
            let tractions = p.tractions(&n, &mut openings);
            for (o, tr) in openings.iter().zip(&tractions) {
                assert!(dot(tr, &n).abs() < 1e-9, "dim {dim}: normal traction");
                assert!(dot(o, &n).abs() < 1e-16, "dim {dim}: normal opening");
                let te = dot(&p.expected(o, &n), &direction);
                assert!((te - dot(tr, &direction)).abs() < 1e-9, "dim {dim}");
            }
            let dis = dissipated(&openings, &tractions);
            assert!(
                (beta * 10. - dis).abs() < 1e-6,
                "dim {dim}: dissipated {dis}"
            );
        }
    }

    #[test]
    fn cycles() {
        for dim in 1..=3 {
            let mut p = Point::new(dim);
            let n = normal(dim);
            let dc = p.delta_c;
            let sigma_c = p.sigma_c;
            for (s, ni) in p.insertion_stress.iter_mut().zip(&n) {
                *s = ni * sigma_c;
            }
            let mut openings = path(
                &n,
                &[
                    (0., 0.1 * dc),
                    (0.1 * dc, 0.),
                    (0., 0.5 * dc),
                    (0.5 * dc, -1e-5),
                    (-1e-5, 0.9 * dc),
                    (0.9 * dc, 0.),
                    (0., dc),
                ],
            );
            let tractions = p.tractions(&n, &mut openings);
            let dis = dissipated(&openings, &tractions);
            // contact dissipation at 0 (upstream tolerance 2e-3)
            assert!((10. - dis).abs() < 2e-3, "dim {dim}: dissipated {dis}");
        }
    }
}
