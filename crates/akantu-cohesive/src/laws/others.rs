// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_cohesive/materials/constitutive_laws/
// material_cohesive_exponential.{hh,cc}, material_cohesive_linear_fatigue.{hh,cc},
// material_cohesive_linear_friction.{hh,cc}, material_cohesive_linear_uncoupled.{hh,cc}.

//! `cohesive_exponential` (CMAT-003), `cohesive_linear_fatigue` (CMAT-004),
//! `cohesive_linear_friction` (CMAT-005), `cohesive_linear_uncoupled`
//! (CMAT-006). Operand order of upstream's expressions; reductions of
//! fixed-size views (`make_view<dim>`) use the halving tree, those of
//! run-time views (`Array::begin(dim)`) sequential sums; `exp` is the
//! glibc-compatible port (`akantu_core::random::glibc_libm`).

use akantu_core::array_shared::SliceRef;
use akantu_core::math::{DEFAULT_TOLERANCE, are_float_equal};
use akantu_core::random::glibc_libm::exp;
use akantu_core::{ElementType, Error, GhostType, Int, Real, Result};
use akantu_model::InternalField;

use super::linear::{Linear, Penetration};
use crate::material::{CohesiveCore, block, block_and_previous_mut, block_mut};
use crate::vecops::{dot_fixed, dot_seq, norm_fixed, std_max, std_min};

const TOL: Real = DEFAULT_TOLERANCE;

fn e1() -> Real {
    exp(1.)
}

/// `cohesive_exponential` (Xu–Needleman-type exponential law).
#[derive(Clone, Debug)]
pub struct Exponential {
    /// `beta`.
    pub beta: Real,
    /// `exponential_penalty`.
    pub exp_penalty: bool,
    /// `contact_tangent`.
    pub contact_tangent: Real,
    /// Upstream `Math::getTolerance()`.
    pub tol: Real,
}

impl Exponential {
    pub(crate) fn new(core: &mut CohesiveCore) -> Self {
        core.use_previous_delta_max = true;
        Self {
            beta: 0.0,
            exp_penalty: true,
            contact_tangent: 1.0,
            tol: TOL,
        }
    }

    /// Upstream `initMaterial`: `contact_tangent` forced to 1 with the
    /// exponential penalty.
    pub(crate) fn after_init(&mut self) {
        if self.exp_penalty && self.contact_tangent != 1. {
            self.contact_tangent = 1.;
        }
    }

    // upstream's per-point helper signature (scalars of one quadrature
    // point); a parameter struct would be rebuilt per point
    #[allow(clippy::too_many_arguments)]
    fn coupled_traction(
        &self,
        dim: usize,
        tract: &mut [Real],
        normal: &[Real],
        delta: Real,
        opening: &[Real],
        delta_max_new: &mut Real,
        delta_max_prev: Real,
        sigma_c: Real,
        delta_c: Real,
    ) {
        if delta.abs() < self.tol {
            tract[..dim].fill(0.0);
            return;
        }
        let beta2 = self.beta * self.beta;
        let non = dot_fixed(opening, normal, dim);
        *delta_max_new = std_max(delta_max_prev, delta);
        for i in 0..dim {
            let op_n_n = normal[i] * (1. - beta2) * non;
            tract[i] =
                (beta2 * opening[i] + op_n_n) * e1() * sigma_c * exp(-*delta_max_new / delta_c)
                    / delta_c;
        }
    }

    fn compressive_traction(
        &self,
        dim: usize,
        tract: &mut [Real],
        normal: &[Real],
        delta_n: Real,
        sigma_c: Real,
        delta_c: Real,
    ) {
        let s = if self.exp_penalty {
            delta_n * e1() * sigma_c * exp(-delta_n / delta_c) / delta_c
        } else {
            self.contact_tangent * e1() * sigma_c * delta_n / delta_c
        };
        for i in 0..dim {
            tract[i] += normal[i] * s;
        }
    }

    /// Upstream `computeTraction(type, ghost)`.
    pub(crate) fn compute_traction(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let dim = core.dim;
        let sigma_c = core.sigma_c_value();
        let delta_c = core.delta_c;
        let prev = if core.delta_max.has_history() {
            core.delta_max.previous(ty, ghost)?.as_slice().to_vec()
        } else {
            // UD-028 compat: upstream reads a null history (crash); the
            // current values stand in for it
            block(&core.delta_max, ty, ghost).to_vec()
        };
        let CohesiveCore {
            tractions,
            opening,
            normals,
            delta_max,
            ..
        } = core;
        let n = block(normals, ty, ghost);
        let nq = n.len() / dim;
        let op = block(opening, ty, ghost);
        let mut tr_g = block_mut(tractions, ty, ghost)?;
        let tr = tr_g.as_mut_slice();
        let mut dm_g = block_mut(delta_max, ty, ghost)?;
        let dm = dm_g.as_mut_slice();
        if op.len() < nq * dim || tr.len() < nq * dim || dm.len() < nq || prev.len() < nq {
            return Err(Error::DimensionMismatch {
                context: "cohesive_exponential: internal shorter than the normals",
                expected: nq,
                got: dm.len().min(prev.len()),
            });
        }
        let beta2 = self.beta * self.beta;
        for q in 0..nq {
            let r = q * dim..(q + 1) * dim;
            let (nr, o) = (&n[r.clone()], &op[r.clone()]);
            let non = dot_fixed(o, nr, dim);
            let mut tang = [0.0; 3];
            for i in 0..dim {
                tang[i] = o[i] - nr[i] * non;
            }
            let tn = norm_fixed(&tang, dim);
            let mut delta = (tn * tn * beta2 + non * non).sqrt();
            if non < 0. && non.abs() > self.tol {
                delta = tn * self.beta;
                self.coupled_traction(
                    dim,
                    &mut tr[r.clone()],
                    nr,
                    delta,
                    &tang,
                    &mut dm[q],
                    prev[q],
                    sigma_c,
                    delta_c,
                );
                self.compressive_traction(dim, &mut tr[r], nr, non, sigma_c, delta_c);
            } else {
                self.coupled_traction(
                    dim, &mut tr[r], nr, delta, o, &mut dm[q], prev[q], sigma_c, delta_c,
                );
            }
        }
        Ok(())
    }

    /// Upstream `computeTangentTraction(type, tangent, ghost)` with the
    /// corrected tangent by default (**UD-248**): the
    /// derivative of [`Exponential::compute_traction`] at the present
    /// opening against the maximal opening the traction reads (its committed
    /// history; without history, UD-028 compat, the current values).
    /// `compat.exponential_tangent`: upstream's expression (ignores `δ_max`,
    /// keeps the normal part of the coupled term in compression).
    pub(crate) fn compute_tangent_traction(
        &self,
        core: &CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
    ) -> Result<()> {
        let dim = core.dim;
        let sigma_c = core.sigma_c_value();
        let delta_c = core.delta_c;
        let beta2 = self.beta * self.beta;
        let upstream = core.compat.exponential_tangent;
        let n = block(&core.normals, ty, ghost);
        let op = block(&core.opening, ty, ghost);
        let nq = n.len() / dim;
        // UD-248: the maximal opening the traction reads at this opening
        let prev_g;
        let prev: &[Real] = if upstream {
            &[]
        } else if core.delta_max.has_history() {
            core.delta_max.previous(ty, ghost)?.as_slice()
        } else {
            prev_g = block(&core.delta_max, ty, ghost);
            &prev_g
        };
        if op.len() < nq * dim || tangent.len() < nq * dim * dim || (!upstream && prev.len() < nq) {
            return Err(Error::DimensionMismatch {
                context: "cohesive_exponential tangent",
                expected: nq * dim,
                got: op
                    .len()
                    .min(if upstream { op.len() } else { prev.len() * dim }),
            });
        }
        for q in 0..nq {
            let r = q * dim..(q + 1) * dim;
            let (nr, o) = (&n[r.clone()], &op[r]);
            let t = &mut tangent[q * dim * dim..(q + 1) * dim * dim];
            let non = dot_fixed(o, nr, dim);
            let mut tang = [0.0; 3];
            for i in 0..dim {
                tang[i] = o[i] - nr[i] * non;
            }
            let tn = norm_fixed(&tang, dim);
            let mut delta = (tn * tn * beta2 + non * non).sqrt();
            if non < 0. && non.abs() > self.tol {
                delta = tn * self.beta;
                if upstream {
                    self.coupled_tangent(dim, t, nr, delta, &tang, sigma_c, delta_c);
                } else {
                    self.coupled_tangent_consistent(
                        dim, t, nr, delta, &tang, true, prev[q], sigma_c, delta_c,
                    );
                }
                let dn = if self.exp_penalty { non } else { 0. };
                let normal_tg = self.contact_tangent
                    * e1()
                    * sigma_c
                    * exp(-dn / delta_c)
                    * (1. - dn / delta_c)
                    / delta_c;
                for j in 0..dim {
                    for i in 0..dim {
                        t[i + dim * j] += nr[i] * nr[j] * normal_tg;
                    }
                }
            } else if upstream {
                self.coupled_tangent(dim, t, nr, delta, o, sigma_c, delta_c);
            } else {
                self.coupled_tangent_consistent(
                    dim, t, nr, delta, o, false, prev[q], sigma_c, delta_c,
                );
            }
        }
        Ok(())
    }

    /// Derivative of the coupled traction (**UD-248** default).
    /// The traction is `T = A·s·g(max(δ_max, δ))`, `A = β²I +
    /// (1−β²) n⊗n`, `g(m) = e σc e^{−m/δc}/δc`, with `s = Δ` and `δ =
    /// √(β²‖Δt‖² + Δn²)` in tension, `s = Δt = P·Δ` (`P = I − n⊗n`) and `δ
    /// = β‖Δt‖` on the compression branch (`compression`), and `T = 0` for
    /// `|δ| < tol`. So `∂T/∂Δ = g·A·∂s/∂Δ − g/δc·(A s)⊗∂δ/∂Δ` on loading
    /// (`δ ≥ δ_max`: also at the transition point, as the linear family),
    /// `g(δ_max)·A·∂s/∂Δ` below `δ_max`, with `δ·∂δ/∂Δ = β²Δt + n Δn` in
    /// tension and `β² P·Δt` in compression. Inside the `|δ| < tol` guard
    /// the branch outside it continues (upstream's `δ := tol`). The tension
    /// loading branch keeps upstream's expression (exact there) bit for bit.
    // the per-point helper's scalars, as `coupled_tangent`
    #[allow(clippy::too_many_arguments)]
    fn coupled_tangent_consistent(
        &self,
        dim: usize,
        t: &mut [Real],
        normal: &[Real],
        delta: Real,
        s: &[Real],
        compression: bool,
        delta_max: Real,
        sigma_c: Real,
        delta_c: Real,
    ) {
        let beta2 = self.beta * self.beta;
        let delta = if delta.abs() < self.tol {
            self.tol
        } else {
            delta
        };
        let loading = delta >= delta_max;
        if loading && !compression {
            // upstream's loading tangent is the derivative there
            self.coupled_tangent(dim, t, normal, delta, s, sigma_c, delta_c);
            return;
        }
        // A·∂s/∂Δ (row i, column j)
        let a_ij = |i: usize, k: usize| {
            (if i == k { beta2 } else { 0.0 }) + (1. - beta2) * normal[i] * normal[k]
        };
        let mut a_ds = [[0.0; 3]; 3];
        for (i, row) in a_ds.iter_mut().enumerate().take(dim) {
            for (j, a) in row.iter_mut().enumerate().take(dim) {
                *a = if compression {
                    (0..dim)
                        .map(|k| {
                            a_ij(i, k) * ((if k == j { 1.0 } else { 0.0 }) - normal[k] * normal[j])
                        })
                        .sum()
                } else {
                    a_ij(i, j)
                };
            }
        }
        if !loading {
            // unloading secant: the traction's g(δ_max), its operation order
            let g = e1() * sigma_c * exp(-delta_max / delta_c) / delta_c;
            for i in 0..dim {
                for j in 0..dim {
                    t[j + dim * i] = a_ds[i][j] * g;
                }
            }
            return;
        }
        // compression, loading: g(δ)·[A P − (A Δt)⊗(β² P Δt)/(δ δc)]
        let sn = dot_fixed(s, normal, dim);
        let mut a_s = [0.0; 3];
        let mut w = [0.0; 3];
        for i in 0..dim {
            a_s[i] = normal[i] * sn * (1. - beta2) + beta2 * s[i];
            w[i] = beta2 * (s[i] - normal[i] * sn);
        }
        let g = e1() * sigma_c * exp(-delta / delta_c) / delta_c;
        for i in 0..dim {
            for j in 0..dim {
                t[j + dim * i] = (a_ds[i][j] - a_s[i] * w[j] / delta / delta_c) * g;
            }
        }
    }

    // upstream's per-point helper signature (scalars of one quadrature
    // point); a parameter struct would be rebuilt per point
    #[allow(clippy::too_many_arguments)]
    fn coupled_tangent(
        &self,
        dim: usize,
        t: &mut [Real],
        normal: &[Real],
        mut delta: Real,
        opening: &[Real],
        sigma_c: Real,
        delta_c: Real,
    ) {
        let beta2 = self.beta * self.beta;
        if delta.abs() < self.tol {
            delta = self.tol;
        }
        let on = dot_fixed(opening, normal, dim);
        let mut de = [0.0; 3];
        for i in 0..dim {
            de[i] = normal[i] * on * (1. - beta2) + beta2 * opening[i];
        }
        let exponent = exp(1. - delta / delta_c) * sigma_c / delta_c;
        for j in 0..dim {
            for i in 0..dim {
                let jij = if i == j { beta2 } else { 0.0 };
                let first = normal[i] * normal[j] * (1. - beta2) + jij;
                let second = de[i] * de[j] / delta / delta_c;
                t[i + dim * j] = (first - second) * exponent;
            }
        }
    }
}

/// `cohesive_linear_fatigue` (Nguyen–Repetto–Ortiz–Radovitzky 2001).
#[derive(Clone, Debug)]
pub struct Fatigue {
    /// The linear part.
    pub linear: Linear,
    /// `delta_f`.
    pub delta_f: Real,
    /// `progressive_delta_f`.
    pub progressive_delta_f: bool,
    /// `count_switches`.
    pub count_switches: bool,
    /// `fatigue_ratio`.
    pub fatigue_ratio: Real,
    /// Previous effective opening.
    pub delta_prec: InternalField<Real>,
    /// Reloading stiffness.
    pub k_plus: InternalField<Real>,
    /// Unloading stiffness.
    pub k_minus: InternalField<Real>,
    /// 1-D traction.
    pub t_1d: InternalField<Real>,
    /// Normal (non-fatigue) regime flag.
    pub normal_regime: InternalField<bool>,
    /// `switches` and `delta_dot_prec` (registered by `initMaterial` when
    /// `count_switches` is set, as upstream).
    pub switch_count: Option<Box<SwitchCount>>,
}

/// Internals of `count_switches` (`cohesive_linear_fatigue`).
#[derive(Clone, Debug)]
pub struct SwitchCount {
    /// Opening/closing switches per quadrature point.
    pub switches: InternalField<Int>,
    /// Previous opening rate δ̇.
    pub delta_dot_prec: InternalField<Real>,
}

impl Fatigue {
    pub(crate) fn new(core: &mut CohesiveCore) -> Result<Self> {
        Ok(Self {
            linear: Linear::new(core)?,
            delta_f: -1.,
            progressive_delta_f: false,
            count_switches: false,
            fatigue_ratio: 1.,
            delta_prec: InternalField::new("delta_prec", 1, 0.0)?,
            k_plus: InternalField::new("K_plus", 1, 0.0)?,
            k_minus: InternalField::new("K_minus", 1, 0.0)?,
            t_1d: InternalField::new("T_1d", 1, 0.0)?,
            normal_regime: InternalField::new("normal_regime", 1, false)?,
            switch_count: None,
        })
    }

    /// Upstream `initMaterial` after the linear part.
    pub(crate) fn after_init(&mut self) -> Result<()> {
        let dce = *self.linear.delta_c_eff.default_value();
        if self.delta_f < 0. {
            self.delta_f = dce;
        } else if self.delta_f < dce {
            return Err(Error::invalid(
                "Delta_f must be greater or equal to delta_c",
            ));
        }
        if self.count_switches {
            // upstream registers both internals here, after the linear
            // initialisation (`registerInternal` initialises them at once on
            // the element filter; they grow with the inserted elements)
            self.switch_count = Some(Box::new(SwitchCount {
                switches: InternalField::new("switches", 1, 0)?,
                delta_dot_prec: InternalField::new("delta_dot_prec", 1, 0.0)?,
            }));
        }
        Ok(())
    }

    /// Upstream `getSwitches(type, ghost)`: the number of sign changes of
    /// the opening rate per quadrature point (`count_switches` only).
    ///
    /// # Errors
    /// `count_switches` not set, or no such block.
    pub fn switches(&self, ty: ElementType, ghost: GhostType) -> Result<SliceRef<'_, Int>> {
        Ok(self
            .switch_count
            .as_ref()
            .ok_or_else(|| Error::invalid("cohesive_linear_fatigue: count_switches is not set"))?
            .switches
            .get(ty, ghost)?
            .into_slice())
    }

    /// Upstream `computeTraction(type, ghost)`.
    pub(crate) fn compute_traction(
        &mut self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let dim = core.dim;
        let l = &self.linear;
        let CohesiveCore {
            tractions,
            opening,
            contact_tractions,
            contact_opening,
            normals,
            delta_max,
            damage,
            ..
        } = core;
        let n = block(normals, ty, ghost);
        let nq = n.len() / dim;
        let mut tr_g = block_mut(tractions, ty, ghost)?;
        let tr = tr_g.as_mut_slice();
        let mut op_g = block_mut(opening, ty, ghost)?;
        let op = op_g.as_mut_slice();
        let mut ct_g = block_mut(contact_tractions, ty, ghost)?;
        let ct = ct_g.as_mut_slice();
        let mut co_g = block_mut(contact_opening, ty, ghost)?;
        let co = co_g.as_mut_slice();
        let mut dm_g = block_mut(delta_max, ty, ghost)?;
        let dm = dm_g.as_mut_slice();
        let mut da_g = block_mut(damage, ty, ghost)?;
        let da = da_g.as_mut_slice();
        let sc = block(&l.sigma_c_eff, ty, ghost);
        let dc = block(&l.delta_c_eff, ty, ghost);
        let is = block(&l.insertion_stress, ty, ghost);
        let mut dp_g = block_mut(&mut self.delta_prec, ty, ghost)?;
        let dp = dp_g.as_mut_slice();
        let mut kp_g = block_mut(&mut self.k_plus, ty, ghost)?;
        let kp = kp_g.as_mut_slice();
        let mut km_g = block_mut(&mut self.k_minus, ty, ghost)?;
        let km = km_g.as_mut_slice();
        let mut t1_g = block_mut(&mut self.t_1d, ty, ghost)?;
        let t1 = t1_g.as_mut_slice();
        let nr_field = &mut self.normal_regime;
        let nb = nr_field.nb_component();
        if !nr_field.exists(ty, ghost) {
            nr_field.values_mut().alloc(0, nb, ty, ghost, false)?;
        }
        let mut nreg_g = nr_field.get_mut(ty, ghost)?;
        let nreg = nreg_g.as_mut_slice();
        let mut sw_g;
        let mut ddp_g;
        let mut switch_arrays = match self.switch_count.as_deref_mut() {
            Some(SwitchCount {
                switches: sw,
                delta_dot_prec: ddp,
            }) => {
                if !sw.exists(ty, ghost) {
                    sw.values_mut().alloc(0, 1, ty, ghost, 0)?;
                }
                sw_g = sw.get_mut(ty, ghost)?;
                let sw = sw_g.as_mut_slice();
                ddp_g = block_mut(ddp, ty, ghost)?;
                let ddp = ddp_g.as_mut_slice();
                if sw.len() < nq || ddp.len() < nq {
                    return Err(Error::DimensionMismatch {
                        context: "cohesive_linear_fatigue: switches shorter than the normals",
                        expected: nq,
                        got: sw.len().min(ddp.len()),
                    });
                }
                Some((sw, ddp))
            }
            None => None,
        };
        let lens = [
            op.len() / dim,
            tr.len() / dim,
            ct.len() / dim,
            co.len() / dim,
            dm.len(),
            da.len(),
            sc.len(),
            dc.len(),
            is.len() / dim,
            dp.len(),
            kp.len(),
            km.len(),
            t1.len(),
            nreg.len(),
        ];
        if lens.iter().any(|&x| x < nq) {
            return Err(Error::DimensionMismatch {
                context: "cohesive_linear_fatigue: internal shorter than the normals",
                expected: nq,
                got: *lens.iter().min().unwrap_or(&0),
            });
        }
        for q in 0..nq {
            let r = q * dim..(q + 1) * dim;
            let nrm = &n[r.clone()];
            // `opening_it->dot(*normal_it)`: run-time views
            let non = dot_seq(&op[r.clone()], nrm, dim);
            let mut no = [0.0; 3];
            let mut to = [0.0; 3];
            for i in 0..dim {
                no[i] = nrm[i] * non;
                to[i] = op[q * dim + i] - no[i];
            }
            let tn = norm_fixed(&to, dim);
            let mut delta = tn * tn * l.beta2_kappa2();
            let mut penetration = non < -l.tol;
            if !l.contact_after_breaking && are_float_equal(da[q], 1., l.tol) {
                penetration = false;
            }
            if penetration {
                for i in 0..dim {
                    ct[q * dim + i] = no[i] * l.penalty;
                    co[q * dim + i] = no[i];
                    op[q * dim + i] = to[i];
                    no[i] = 0.0;
                }
            } else {
                delta += non * non;
                ct[r.clone()].fill(0.0);
                co[r.clone()].fill(0.0);
            }
            delta = delta.sqrt();
            dm[q] = std_max(delta, dm[q]);
            da[q] = std_min(dm[q] / dc[q], 1.);
            let delta_dot = delta - dp[q];
            // count switches if asked
            if let Some((sw, ddp)) = switch_arrays.as_mut()
                && ((delta_dot > 0. && ddp[q] <= 0.) || (delta_dot < 0. && ddp[q] >= 0.))
            {
                sw[q] += 1;
            }
            if let Some((_, ddp)) = switch_arrays.as_mut() {
                ddp[q] = delta_dot;
            }
            if self.progressive_delta_f {
                self.delta_f = dm[q];
            }
            let tr_q = &mut tr[r.clone()];
            if are_float_equal(da[q], 1., l.tol) {
                tr_q.fill(0.0);
            } else if are_float_equal(da[q], 0., l.tol) {
                if penetration {
                    tr_q.fill(0.0);
                } else {
                    tr_q.copy_from_slice(&is[r.clone()]);
                }
                t1[q] = sc[q];
            } else if delta <= l.tol {
                tr_q.fill(0.0);
            } else if delta_dot.abs() > l.tol {
                if delta_dot > 0. {
                    if !nreg[q] {
                        kp[q] *= 1. - delta_dot / self.delta_f;
                        t1[q] += kp[q] * delta_dot;
                        let max_traction = sc[q] * (1. - delta / dc[q]);
                        let exceeded = t1[q] > max_traction;
                        if exceeded {
                            t1[q] = max_traction;
                        }
                        if dm[q] > self.fatigue_ratio * dc[q] {
                            dm[q] = sc[q] / (t1[q] / delta + sc[q] / dc[q]);
                            da[q] = std_min(dm[q] / dc[q], 1.);
                            km[q] = sc[q] / dm[q] * (1. - da[q]);
                            nreg[q] = true;
                        } else {
                            km[q] = t1[q] / delta;
                            if exceeded {
                                kp[q] = km[q];
                            }
                        }
                    } else {
                        km[q] = sc[q] / dm[q] * (1. - da[q]);
                    }
                } else if !nreg[q] {
                    kp[q] += (kp[q] - km[q]) * delta_dot / self.delta_f;
                    t1[q] = km[q] * delta;
                }
                for i in 0..dim {
                    let mut v = to[i];
                    v *= l.beta2_kappa();
                    v += no[i];
                    v *= km[q];
                    tr_q[i] = v;
                }
            }
            dp[q] = delta;
        }
        Ok(())
    }
}

/// `cohesive_linear_friction`: Coulomb friction under penetration.
#[derive(Clone, Debug)]
pub struct Friction {
    /// The linear part.
    pub linear: Linear,
    /// `mu`.
    pub mu_max: Real,
    /// `penalty_for_friction`.
    pub friction_penalty: Real,
    /// Residual sliding (history).
    pub residual_sliding: InternalField<Real>,
    /// Friction force.
    pub friction_force: InternalField<Real>,
}

impl Friction {
    pub(crate) fn new(core: &mut CohesiveCore) -> Result<Self> {
        let mut residual_sliding = InternalField::new("residual_sliding", 1, 0.0)?;
        residual_sliding.initialize_history();
        Ok(Self {
            linear: Linear::new(core)?,
            mu_max: 0.,
            friction_penalty: 0.,
            residual_sliding,
            friction_force: InternalField::new("friction_force", core.dim, 0.0)?,
        })
    }

    /// Implicit evaluation reading the converged history (**UD-247**
    /// default): an implicit run with the `δ_max` history, without
    /// `compat.friction_upstream_implicit`.
    fn converged_history(core: &CohesiveCore, explicit: bool) -> bool {
        !explicit && core.delta_max.has_history() && !core.compat.friction_upstream_implicit
    }

    /// Upstream `computeTraction(type, ghost)`: implicit models restart
    /// from the previous `δ_max`, then the linear law plus friction.
    ///
    /// Per penetrating point, with `x = ‖Δt‖`, `s_p` the committed residual
    /// sliding, `d = x − s_p`, `k = k_f` and `τ_max = μ·penalty·|Δn|`:
    /// `τ = sign(d)·min(k|d|, τ_max)` and the friction force `τ Δt/x`.
    ///
    /// * **UD-249** (B): the residual sliding is `s = x −
    ///   τ/k` with the signed `τ` (the play operator `clamp(s_p, x − τ_max/k,
    ///   x + τ_max/k)`); `compat.friction_residual_sliding_abs`: upstream's
    ///   `x − |τ|/k`.
    /// * **UD-225** (A): at `x = 0` exactly the direction `Δt/x` is
    ///   undefined (upstream 0/0 = NaN, always in 1-D): the friction force is
    ///   zero; the scalar residual sliding is updated as elsewhere.
    /// * **UD-247** (B): an implicit evaluation decides contact from the
    ///   converged damage `min(δ_max previous/δc, 1)` (upstream: the damage
    ///   the previous trial left), so a trial depends only on its opening and
    ///   the committed state; `compat.friction_upstream_implicit`: upstream.
    ///   Its maximal opening is `max(δ_max previous, δ)` in both modes.
    pub(crate) fn compute_traction(
        &mut self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        explicit: bool,
    ) -> Result<()> {
        let dim = core.dim;
        let converged = Self::converged_history(core, explicit);
        let abs_update = core.compat.friction_residual_sliding_abs;
        if !converged && !explicit && core.delta_max.has_history() {
            let prev = core.delta_max.previous(ty, ghost)?.as_slice().to_vec();
            block_mut(&mut core.delta_max, ty, ghost)?
                .as_mut_slice()
                .copy_from_slice(&prev);
        }
        let Self {
            linear,
            mu_max,
            friction_penalty,
            residual_sliding,
            friction_force,
        } = self;
        let prev_rs = residual_sliding
            .previous(ty, ghost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default();
        let mut rs_g = block_mut(residual_sliding, ty, ghost)?;
        let rs = rs_g.as_mut_slice();
        let mut ff_g = block_mut(friction_force, ty, ghost)?;
        let ff = ff_g.as_mut_slice();
        let nq = block(&core.normals, ty, ghost).len() / dim;
        if rs.len() < nq || ff.len() < nq * dim || prev_rs.len() < nq {
            return Err(Error::DimensionMismatch {
                context: "cohesive_linear_friction: internal shorter than the normals",
                expected: nq,
                got: rs.len(),
            });
        }
        let mut q = 0;
        let (mu, fp, pen) = (*mu_max, *friction_penalty, linear.penalty);
        // UD-241 does not apply: implicit runs restart from the previous
        // `δ_max` (upstream: the copy above; UD-247 default: the converged
        // history, the same maximal opening), explicit ones evaluate once
        // per step
        linear.compute_traction_history(core, ty, ghost, 0.0, converged, &mut |st, traction| {
            let fq = &mut ff[q * dim..(q + 1) * dim];
            if !st.penetration {
                fq.fill(0.0);
                q += 1;
                return;
            }
            let x = st.tangential_opening_norm;
            let tau_max = mu * pen * st.normal_opening_norm.abs();
            let dsn = (x - prev_rs[q]).abs();
            let mut tau = std_min(fp * dsn, tau_max);
            if (x - prev_rs[q]) < 0.0 {
                tau = -tau;
            }
            if x == 0.0 {
                // UD-225 (A): no direction at zero tangential opening
                fq.fill(0.0);
            } else {
                for (f, t) in fq.iter_mut().zip(&st.tangential_opening) {
                    *f = tau * t / x;
                }
            }
            rs[q] = if fp == 0. {
                x
            } else if abs_update {
                // UD-249 compat
                x - (tau.abs() / fp)
            } else {
                x - (tau / fp)
            };
            for i in 0..dim {
                traction[i] += fq[i];
            }
            q += 1;
        })
    }

    /// Upstream `computeTangentTraction`: the linear tangent plus the
    /// friction part.
    ///
    /// **UD-247** default: the derivative of [`Friction::compute_traction`]
    /// at the present opening (the linear part from the same history and
    /// contact decision as the traction: the converged ones in an implicit
    /// run). With `t̂ = Δt/x`, `P⊥ = I − n⊗n − t̂⊗t̂` (zero in 2-D) and the
    /// traction's `d`, `τ` and stick test `k|d| < τ_max` from the present
    /// normal opening, `∂f/∂Δ` is `k t̂⊗t̂ + τ/x·P⊥` while sticking and
    /// `τ/x·P⊥ − sign(d)·μ·penalty·t̂⊗n` while slipping (`∂τ_max/∂Δ =
    /// −μ·penalty·n` for `Δn < 0`). At `x = 0` (**UD-225**: zero force) the
    /// limit for `s_p = 0` is used: `k (I − n⊗n)` while sticking, 0 while
    /// slipping. `compat.friction_upstream_implicit`: upstream's linear
    /// tangent (current `δ_max` and damage) plus `k (I − n⊗n)` when
    /// `min(k|x − s_p|, τ_max) < τ_max` and `τ_max > tol`, `τ_max` from the
    /// previous step's normal opening.
    pub(crate) fn compute_tangent_traction(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
    ) -> Result<()> {
        if !core.compat.friction_upstream_implicit {
            return self.compute_tangent_traction_exact(core, ty, ghost, tangent);
        }
        let dim = core.dim;
        let prev_op = core
            .opening
            .previous(ty, ghost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default();
        let prev_rs = self
            .residual_sliding
            .previous(ty, ghost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default();
        let normals = block(&core.normals, ty, ghost).to_vec();
        let nq = normals.len() / dim;
        if prev_op.len() < nq * dim || prev_rs.len() < nq {
            return Err(Error::DimensionMismatch {
                context: "cohesive_linear_friction tangent",
                expected: nq,
                got: prev_rs.len(),
            });
        }
        let mut q = 0;
        let (mu, fp, pen, tol) = (
            self.mu_max,
            self.friction_penalty,
            self.linear.penalty,
            self.linear.tol,
        );
        self.linear.compute_tangent_traction_with(
            core,
            ty,
            ghost,
            tangent,
            None,
            Penetration::Relative,
            &mut |st, t| {
                if st.penetration {
                    let n = &normals[q * dim..(q + 1) * dim];
                    let pnon = std_min(dot_fixed(&prev_op[q * dim..(q + 1) * dim], n, dim), 0.);
                    let tau_max = mu * pen * pnon.abs();
                    let dsn = (st.tangential_opening_norm - prev_rs[q]).abs();
                    let tau = std_min(fp * dsn, tau_max);
                    if tau < tau_max && tau_max > tol {
                        for j in 0..dim {
                            for i in 0..dim {
                                let id = if i == j { 1.0 } else { 0.0 };
                                t[i + dim * j] += (id - n[i] * n[j]) * fp;
                            }
                        }
                    }
                }
                q += 1;
            },
        )
    }

    /// The UD-247 default of [`Friction::compute_tangent_traction`].
    fn compute_tangent_traction_exact(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
    ) -> Result<()> {
        let dim = core.dim;
        let converged = Self::converged_history(core, core.default_solver_explicit);
        let prev_rs = self
            .residual_sliding
            .previous(ty, ghost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default();
        let normals = block(&core.normals, ty, ghost).to_vec();
        let nq = normals.len() / dim;
        if prev_rs.len() < nq {
            return Err(Error::DimensionMismatch {
                context: "cohesive_linear_friction tangent",
                expected: nq,
                got: prev_rs.len(),
            });
        }
        let mut q = 0;
        let (mu, fp, pen) = (self.mu_max, self.friction_penalty, self.linear.penalty);
        self.linear.compute_tangent_traction_history(
            core,
            ty,
            ghost,
            tangent,
            0.0,
            converged,
            Penetration::Relative,
            &mut |st, t| {
                if st.penetration {
                    let n = &normals[q * dim..(q + 1) * dim];
                    friction_tangent_on_quad(dim, t, n, st, prev_rs[q], mu, fp, pen);
                }
                q += 1;
            },
        )
    }
}

/// The derivative of the friction force `τ Δt/x` of one penetrating point
/// (see [`Friction::compute_tangent_traction`]), added to `t`
/// (`t[j + dim·i] += ∂f_i/∂Δ_j`).
// the per-point scalars of the friction law (one quadrature point)
#[allow(clippy::too_many_arguments)]
fn friction_tangent_on_quad(
    dim: usize,
    t: &mut [Real],
    n: &[Real],
    st: &super::linear::QuadState,
    prev_rs: Real,
    mu: Real,
    fp: Real,
    pen: Real,
) {
    let x = st.tangential_opening_norm;
    let tau_max = mu * pen * st.normal_opening_norm.abs();
    let d = x - prev_rs;
    let stick = fp * d.abs() < tau_max;
    if x == 0.0 {
        // UD-225: zero force at x = 0; the stick stiffness of `s_p = 0`
        if stick {
            for i in 0..dim {
                for j in 0..dim {
                    let id = if i == j { 1.0 } else { 0.0 };
                    t[j + dim * i] += (id - n[i] * n[j]) * fp;
                }
            }
        }
        return;
    }
    let mut th = [0.0; 3];
    for (t, o) in th.iter_mut().zip(&st.tangential_opening).take(dim) {
        *t = o / x;
    }
    let sign = if d < 0.0 { -1.0 } else { 1.0 };
    let tau = if stick { fp * d } else { sign * tau_max };
    let tau_x = tau / x;
    let slip_n = sign * mu * pen;
    for i in 0..dim {
        for j in 0..dim {
            // the direction's derivative P⊥/x (I − n⊗n = t̂⊗t̂ in 2-D)
            let perp = if dim == 3 {
                let id = if i == j { 1.0 } else { 0.0 };
                (id - n[i] * n[j] - th[i] * th[j]) * tau_x
            } else {
                0.0
            };
            let along = if stick {
                fp * th[i] * th[j]
            } else {
                -(slip_n * th[i] * n[j])
            };
            t[j + dim * i] += perp + along;
        }
    }
}

/// `cohesive_linear_uncoupled`: separate normal and shear damage.
#[derive(Clone, Debug)]
pub struct Uncoupled {
    /// The linear part.
    pub linear: Linear,
    /// `roughness`.
    pub roughness: Real,
    /// Maximal normal opening (history).
    pub delta_n_max: InternalField<Real>,
    /// Maximal shear opening (history).
    pub delta_t_max: InternalField<Real>,
    /// Normal damage.
    pub damage_n: InternalField<Real>,
    /// Shear damage.
    pub damage_t: InternalField<Real>,
}

impl Uncoupled {
    pub(crate) fn new(core: &mut CohesiveCore) -> Result<Self> {
        let mut delta_n_max = InternalField::new("delta_n_max", 1, 0.0)?;
        delta_n_max.initialize_history();
        let mut delta_t_max = InternalField::new("delta_t_max", 1, 0.0)?;
        delta_t_max.initialize_history();
        Ok(Self {
            linear: Linear::new(core)?,
            roughness: 1.,
            delta_n_max,
            delta_t_max,
            damage_n: InternalField::new("damage_n", 1, 0.0)?,
            damage_t: InternalField::new("damage_t", 1, 0.0)?,
        })
    }

    /// Upstream `computeTraction(type, ghost)`.
    ///
    /// **UD-241**: upstream raises the current `δn,max`
    /// and `δt,max` at every evaluation while its tangent reads their
    /// previous (converged) values; by default the traction uses the
    /// converged maximal openings too, `max(previous, δ)`
    /// (`compat.delta_max_per_iterate` reproduces upstream).
    pub(crate) fn compute_traction(
        &mut self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let dim = core.dim;
        let converged = !core.compat.delta_max_per_iterate;
        let l = &self.linear;
        let r2 = self.roughness;
        let CohesiveCore {
            tractions,
            opening,
            contact_tractions,
            contact_opening,
            normals,
            ..
        } = core;
        let n = block(normals, ty, ghost);
        let nq = n.len() / dim;
        let mut tr_g = block_mut(tractions, ty, ghost)?;
        let tr = tr_g.as_mut_slice();
        let op = block(opening, ty, ghost);
        let mut ct_g = block_mut(contact_tractions, ty, ghost)?;
        let ct = ct_g.as_mut_slice();
        let mut co_g = block_mut(contact_opening, ty, ghost)?;
        let co = co_g.as_mut_slice();
        let sc = block(&l.sigma_c_eff, ty, ghost);
        let dc = block(&l.delta_c_eff, ty, ghost);
        let is = block(&l.insertion_stress, ty, ghost);
        let ctx = "cohesive_linear_uncoupled: previous maximal opening shorter than the normals \
                   (UD-241)";
        let (mut dnm_g, dnm_prev) =
            block_and_previous_mut(&mut self.delta_n_max, ty, ghost, nq, converged, ctx)?;
        let dnm = dnm_g.as_mut_slice();
        let (mut dtm_g, dtm_prev) =
            block_and_previous_mut(&mut self.delta_t_max, ty, ghost, nq, converged, ctx)?;
        let dtm = dtm_g.as_mut_slice();
        let mut dn_guard = block_mut(&mut self.damage_n, ty, ghost)?;
        let dn_ = dn_guard.as_mut_slice();
        let mut dt_guard = block_mut(&mut self.damage_t, ty, ghost)?;
        let dt_ = dt_guard.as_mut_slice();
        let lens = [
            tr.len() / dim,
            op.len() / dim,
            ct.len() / dim,
            co.len() / dim,
            sc.len(),
            dc.len(),
            is.len() / dim,
            dnm.len(),
            dtm.len(),
            dn_.len(),
            dt_.len(),
        ];
        if lens.iter().any(|&x| x < nq) {
            return Err(Error::DimensionMismatch {
                context: "cohesive_linear_uncoupled: internal shorter than the normals",
                expected: nq,
                got: *lens.iter().min().unwrap_or(&0),
            });
        }
        for q in 0..nq {
            let r = q * dim..(q + 1) * dim;
            let nrm = &n[r.clone()];
            let dc2_r2 = dc[q] * dc[q] / r2 / r2;
            let non = dot_seq(&op[r.clone()], nrm, dim);
            let mut no = [0.0; 3];
            let mut to = [0.0; 3];
            for i in 0..dim {
                no[i] = nrm[i] * non;
                to[i] = op[q * dim + i] - no[i];
            }
            let tn = norm_fixed(&to, dim);
            let mut delta_n = tn * tn * l.beta2_kappa2();
            let mut delta_t = tn * tn * l.beta2_kappa2();
            let mut penetration = non < 0.0;
            // With the converged history the contact is
            // disabled by the converged normal damage, not by the damage the
            // previous trial left
            let contact_damage = dnm_prev.map_or(dn_[q], |p| std_min(p[q] / dc[q], 1.));
            if !l.contact_after_breaking && are_float_equal(contact_damage, 1., l.tol) {
                penetration = false;
            }
            if penetration {
                for i in 0..dim {
                    ct[q * dim + i] = no[i] * l.penalty;
                    co[q * dim + i] = no[i];
                    no[i] = 0.0;
                }
            } else {
                delta_n += non * non;
                delta_t += non * non * dc2_r2;
                ct[r.clone()].fill(0.0);
                co[r.clone()].fill(0.0);
            }
            delta_n = delta_n.sqrt();
            delta_t = delta_t.sqrt();
            dnm[q] = std_max(dnm_prev.map_or(dnm[q], |p| p[q]), delta_n);
            dn_[q] = std_min(dnm[q] / dc[q], 1.);
            dtm[q] = std_max(dtm_prev.map_or(dtm[q], |p| p[q]), delta_t);
            dt_[q] = std_min(dtm[q] / dc[q], 1.);
            let mut nt = [0.0; 3];
            let mut st = [0.0; 3];
            if are_float_equal(dn_[q], 1., l.tol) {
            } else if are_float_equal(dn_[q], 0., l.tol) {
                if !penetration {
                    nt[..dim].copy_from_slice(&is[r.clone()]);
                }
            } else {
                for i in 0..dim {
                    nt[i] = no[i] * sc[q] / dnm[q] * (1. - dn_[q]);
                }
            }
            if !(are_float_equal(dt_[q], 1., l.tol) || are_float_equal(dt_[q], 0., l.tol)) {
                for i in 0..dim {
                    st[i] = to[i] * l.beta2_kappa() * sc[q] / dtm[q] * (1. - dt_[q]);
                }
            }
            for i in 0..dim {
                tr[q * dim + i] = nt[i] + st[i];
            }
        }
        Ok(())
    }

    /// Upstream `computeTangentTraction(type, tangent, ghost)` (it reads the
    /// previous `δn,max`, `δt,max`). With the converged history (UD-241
    /// default) the contact is disabled by the converged normal damage, as
    /// in the traction. **UD-244** (default: the
    /// derivative of the traction; `compat.uncoupled_tangent`: upstream):
    /// the shear term `β²/κ (I − n⊗n)·T/δt` (upstream lacks `β²/κ`); the
    /// opening is the traction's (upstream adds the contact opening back,
    /// doubling the normal part on penetration and modifying the internal)
    /// and on penetration the shear derivative uses the tangential opening.
    pub(crate) fn compute_tangent_traction(
        &self,
        core: &mut CohesiveCore,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
    ) -> Result<()> {
        let dim = core.dim;
        let l = &self.linear;
        let r2 = self.roughness;
        let upstream = core.compat.uncoupled_tangent;
        let converged = !core.compat.delta_max_per_iterate;
        let dnm = self.delta_n_max.previous(ty, ghost)?.as_slice();
        let dtm = self.delta_t_max.previous(ty, ghost)?.as_slice();
        let dn_ = block(&self.damage_n, ty, ghost);
        let CohesiveCore {
            opening,
            contact_opening,
            normals,
            ..
        } = core;
        let n = block(normals, ty, ghost);
        let nq = n.len() / dim;
        let mut op_g = block_mut(opening, ty, ghost)?;
        let op = op_g.as_mut_slice();
        let co = block(contact_opening, ty, ghost);
        let sc = block(&l.sigma_c_eff, ty, ghost);
        let dc = block(&l.delta_c_eff, ty, ghost);
        let lens = [
            op.len() / dim,
            co.len() / dim,
            sc.len(),
            dc.len(),
            dnm.len(),
            dtm.len(),
            dn_.len(),
            tangent.len() / (dim * dim),
        ];
        if lens.iter().any(|&x| x < nq) {
            return Err(Error::DimensionMismatch {
                context: "cohesive_linear_uncoupled tangent",
                expected: nq,
                got: *lens.iter().min().unwrap_or(&0),
            });
        }
        for q in 0..nq {
            let r = q * dim..(q + 1) * dim;
            let nrm = &n[r.clone()];
            let t = &mut tangent[q * dim * dim..(q + 1) * dim * dim];
            let dc2_r2 = dc[q] * dc[q] / r2 / r2;
            if upstream {
                for i in 0..dim {
                    op[q * dim + i] += co[q * dim + i];
                }
            }
            let o: Vec<Real> = op[r.clone()].to_vec();
            let non = dot_seq(&o, nrm, dim);
            let mut no = [0.0; 3];
            let mut to = [0.0; 3];
            for i in 0..dim {
                no[i] = nrm[i] * non;
                to[i] = o[i] - no[i];
            }
            let tn = norm_fixed(&to, dim);
            let mut delta_n = tn * tn * l.beta2_kappa2();
            let mut delta_t = tn * tn * l.beta2_kappa2();
            let mut penetration = non < 0.0;
            let contact_damage = if converged {
                std_min(dnm[q] / dc[q], 1.)
            } else {
                dn_[q]
            };
            if !l.contact_after_breaking && are_float_equal(contact_damage, 1., l.tol) {
                penetration = false;
            }
            let nout = |i: usize, j: usize| nrm[i] * nrm[j];
            if penetration {
                for j in 0..dim {
                    for i in 0..dim {
                        t[i + dim * j] = nout(i, j) * l.penalty;
                    }
                }
                no = [0.0; 3];
            } else {
                delta_n += non * non;
                delta_n = delta_n.sqrt();
                delta_t += non * non * dc2_r2;
                if delta_n < l.tol {
                    delta_n = dc[q] / 1000.;
                }
                let (mut derivative, mut tt) = (0., 0.);
                if delta_n >= dnm[q] {
                    if delta_n <= dc[q] {
                        derivative = -sc[q] / (delta_n * delta_n);
                        tt = sc[q] * (1. - delta_n / dc[q]);
                    }
                } else if delta_n < dnm[q] {
                    let t_max = sc[q] * (1. - dnm[q] / dc[q]);
                    tt = t_max / dnm[q] * delta_n;
                }
                let mut dtil = [0.0; 3];
                for i in 0..dim {
                    dtil[i] = no[i] * (1. - l.beta2_kappa2()) + o[i] * l.beta2_kappa2();
                }
                for j in 0..dim {
                    for i in 0..dim {
                        let nn = nout(i, j) * tt / delta_n;
                        // prov(i, j); tangent = provᵀ
                        let prov = no[i] * dtil[j] * derivative / delta_n + nn;
                        t[j + dim * i] = prov;
                    }
                }
            }
            let (mut derivative, mut tt) = (0., 0.);
            delta_t = delta_t.sqrt();
            if delta_t < l.tol {
                delta_t = dc[q] / 1000.;
            }
            if delta_t >= dtm[q] {
                if delta_t <= dc[q] {
                    derivative = -sc[q] / (delta_t * delta_t);
                    tt = sc[q] * (1. - delta_t / dc[q]);
                }
            } else if delta_t < dtm[q] {
                let t_max = sc[q] * (1. - dtm[q] / dc[q]);
                tt = t_max / dtm[q] * delta_t;
            }
            let mut dtil = [0.0; 3];
            let mut dhat = [0.0; 3];
            // UD-244: on penetration δt has no normal part
            let o_shear = if penetration && !upstream {
                &to[..]
            } else {
                &o[..]
            };
            for i in 0..dim {
                dtil[i] = no[i] * (dc2_r2 - l.beta2_kappa2()) + o_shear[i] * l.beta2_kappa2();
                dhat[i] = to[i] * l.beta2_kappa();
            }
            let shear_factor = if upstream { 1.0 } else { l.beta2_kappa() };
            for j in 0..dim {
                for i in 0..dim {
                    let id = if i == j { 1.0 } else { 0.0 };
                    let ii = (id - nout(i, j)) * shear_factor * (tt / delta_t);
                    let prov = dhat[i] * dtil[j] * derivative / delta_t + ii;
                    t[j + dim * i] += prov;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::block_mut;

    const TY: ElementType = ElementType::Cohesive2d4;
    const G: GhostType = GhostType::NotGhost;

    fn set(f: &mut InternalField<Real>, v: &[Real]) {
        let mut a = block_mut(f, TY, G).unwrap();
        a.try_resize(v.len() / a.nb_component(), 0.0).unwrap();
        a.as_mut_slice().copy_from_slice(v);
    }

    /// UD-241: `cohesive_linear_uncoupled` raised its
    /// current `δn,max` at every evaluation (its tangent reads the previous
    /// value); by default an overshooting Newton iterate (Δn = 0.3) leaves
    /// no normal damage at the converged opening (Δn = 0.2): `δn,max =
    /// max(previous 0.1, 0.2)`. Upstream (per iterate) keeps 0.3.
    #[test]
    fn ud241_uncoupled_iterate_leaves_no_damage() {
        for per_iterate in [false, true] {
            let mut core = CohesiveCore::new(2).unwrap();
            core.compat.delta_max_per_iterate = per_iterate;
            let mut u = Uncoupled::new(&mut core).unwrap();
            set(&mut core.normals, &[0., 1.]);
            set(&mut core.tractions, &[0., 0.]);
            set(&mut core.contact_tractions, &[0., 0.]);
            set(&mut core.contact_opening, &[0., 0.]);
            set(&mut u.linear.sigma_c_eff, &[2.0]);
            set(&mut u.linear.delta_c_eff, &[0.5]);
            set(&mut u.linear.insertion_stress, &[0., 0.]);
            set(&mut u.damage_n, &[0.]);
            set(&mut u.damage_t, &[0.]);
            for f in [&mut u.delta_n_max, &mut u.delta_t_max] {
                set(f, &[0.1]);
                f.save_current_values().unwrap();
            }
            for opening in [[0.0, 0.3], [0.0, 0.2]] {
                set(&mut core.opening, &opening);
                u.compute_traction(&mut core, TY, G).unwrap();
            }
            let dn = u.delta_n_max.get(TY, G).unwrap().as_slice()[0];
            let d = u.damage_n.get(TY, G).unwrap().as_slice()[0];
            let t = block(&core.tractions, TY, G)[1];
            let (dn_e, d_e) = if per_iterate { (0.3, 0.6) } else { (0.2, 0.4) };
            assert_eq!(dn, dn_e, "per_iterate {per_iterate}");
            assert!((d - d_e).abs() <= 1e-15, "per_iterate {per_iterate}: {d}");
            // normal traction Δn σc,eff/δn,max (1 − dn)
            let t_e = 0.2 * 2.0 / dn_e * (1. - d_e);
            assert!(
                (t - t_e).abs() <= 1e-15,
                "per_iterate {per_iterate}: {t} vs {t_e}"
            );
        }
    }

    /// An uncoupled point (normal (0, 1), `σc,eff = 2`, `δc,eff = 0.5`, `β =
    /// 0.8`, `κ = 1.5`: `β²/κ ≠ 1`, penalty 50) whose converged maximal
    /// openings are `δn,max = δt,max = 0.1` (saved).
    fn uncoupled_point() -> (CohesiveCore, Uncoupled) {
        let mut core = CohesiveCore::new(2).unwrap();
        let mut u = Uncoupled::new(&mut core).unwrap();
        u.linear.beta = 0.8;
        u.linear.kappa = 1.5;
        u.linear.penalty = 50.;
        u.linear.update_internal_parameters();
        set(&mut core.normals, &[0., 1.]);
        set(&mut core.tractions, &[0., 0.]);
        set(&mut core.contact_tractions, &[0., 0.]);
        set(&mut core.contact_opening, &[0., 0.]);
        set(&mut core.opening, &[0., 0.]);
        set(&mut u.linear.sigma_c_eff, &[2.0]);
        set(&mut u.linear.delta_c_eff, &[0.5]);
        set(&mut u.linear.insertion_stress, &[0., 0.]);
        set(&mut u.damage_n, &[0.]);
        set(&mut u.damage_t, &[0.]);
        for f in [&mut u.delta_n_max, &mut u.delta_t_max] {
            set(f, &[0.1]);
            f.save_current_values().unwrap();
        }
        (core, u)
    }

    fn total(core: &mut CohesiveCore, u: &mut Uncoupled, o: [Real; 2]) -> [Real; 2] {
        set(&mut core.opening, &o);
        u.compute_traction(core, TY, G).unwrap();
        let t = block(&core.tractions, TY, G);
        let c = block(&core.contact_tractions, TY, G);
        [t[0] + c[0], t[1] + c[1]]
    }

    /// `(tangent, fd)` at `opening`, every evaluation preceded by the trial
    /// `before`; `tangent[j + 2i] = ∂T_i/∂Δ_j`.
    fn tangent_and_fd(
        core: &mut CohesiveCore,
        u: &mut Uncoupled,
        opening: [Real; 2],
        before: [Real; 2],
    ) -> ([Real; 4], [[Real; 2]; 2]) {
        let h = 1e-7;
        let mut fd = [[0.0; 2]; 2];
        for j in 0..2 {
            let (mut p, mut m) = (opening, opening);
            p[j] += h;
            m[j] -= h;
            let _ = total(core, u, before);
            let tp = total(core, u, p);
            let _ = total(core, u, before);
            let tm = total(core, u, m);
            for i in 0..2 {
                fd[i][j] = (tp[i] - tm[i]) / (2. * h);
            }
        }
        let _ = total(core, u, before);
        let _ = total(core, u, opening);
        let mut k = [0.0; 4];
        u.compute_tangent_traction(core, TY, G, &mut k).unwrap();
        (k, fd)
    }

    fn matches(k: &[Real; 4], fd: &[[Real; 2]; 2]) -> bool {
        (0..2).all(|i| {
            (0..2).all(|j| (k[j + 2 * i] - fd[i][j]).abs() <= 1e-6 * fd[i][j].abs().max(1.))
        })
    }

    /// `cohesive_linear_uncoupled`: after a trial past
    /// complete normal failure (Δn = 0.6) a compression trial of a point
    /// whose converged state is intact takes the contact branch (both
    /// `contact_after_breaking` settings), and the default tangent is the
    /// derivative of the trial traction there — on the shear unloading
    /// secant (Δt = 0.03) and on shear loading (Δt = 0.25, where the
    /// derivative term reads the tangential opening). Upstream's tangent
    /// (UD-244 compat) lacks the shear `β²/κ` and doubles the normal
    /// opening.
    #[test]
    fn co3_01_uncoupled_contact_branch_after_an_overshooting_trial() {
        let overshoot = [0.0, 0.6];
        for cab in [false, true] {
            for opening in [[0.03, -0.02], [0.25, -0.02]] {
                let (mut core, mut u) = uncoupled_point();
                u.linear.contact_after_breaking = cab;
                let _ = total(&mut core, &mut u, overshoot);
                assert_eq!(block(&u.damage_n, TY, G), &[1.0]);
                let _ = total(&mut core, &mut u, opening);
                assert_eq!(
                    block(&core.contact_tractions, TY, G)[1],
                    -0.02 * 50.,
                    "cab {cab} {opening:?}: penalty contact"
                );
                let (k, fd) = tangent_and_fd(&mut core, &mut u, opening, overshoot);
                assert!(
                    matches(&k, &fd),
                    "cab {cab} {opening:?}: tangent {k:?} vs finite differences {fd:?}"
                );
                // the tangent leaves the opening internal as the traction did
                assert_eq!(block(&core.opening, TY, G), &opening);
                core.compat.uncoupled_tangent = true;
                let (kc, _) = tangent_and_fd(&mut core, &mut u, opening, overshoot);
                assert!(!matches(&kc, &fd), "cab {cab} {opening:?}: compat {kc:?}");
            }
        }
    }

    /// An exponential point of dimension `dim` (normal `n`, `σc = 2`, `δc =
    /// 0.5`, `β = 0.8`: mixed mode) with the `δ_max` history (UD-028
    /// default) whose committed maximal opening is `prev`.
    fn exponential_point(dim: usize, prev: Real) -> (CohesiveCore, Exponential, Vec<Real>) {
        let mut core = CohesiveCore::new(dim).unwrap();
        let mut e = Exponential::new(&mut core);
        e.beta = 0.8;
        core.delta_c = 0.5;
        core.sigma_c
            .set_random_distribution(akantu_core::random::RandomParameter::new(2.0));
        core.delta_max.initialize_history();
        let n: Vec<Real> = if dim == 2 {
            vec![0.6, 0.8]
        } else {
            let v: [Real; 3] = [0.36, 0.48, 0.8];
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            v.iter().map(|x| x / l).collect()
        };
        let ty = if dim == 2 {
            TY
        } else {
            ElementType::Cohesive3d6
        };
        for (f, v) in [
            (&mut core.normals, n.clone()),
            (&mut core.tractions, vec![0.0; dim]),
            (&mut core.opening, vec![0.0; dim]),
            (&mut core.delta_max, vec![prev]),
        ] {
            let mut a = block_mut(f, ty, G).unwrap();
            a.try_resize(v.len() / a.nb_component(), 0.0).unwrap();
            a.as_mut_slice().copy_from_slice(&v);
        }
        core.delta_max.save_current_values().unwrap();
        (core, e, n)
    }

    fn exp_ty(dim: usize) -> ElementType {
        if dim == 2 {
            TY
        } else {
            ElementType::Cohesive3d6
        }
    }

    /// Traction of the exponential point at opening `o` (a trial: the
    /// committed `δ_max` is unchanged).
    fn exp_traction(core: &mut CohesiveCore, e: &Exponential, o: &[Real]) -> Vec<Real> {
        let ty = exp_ty(core.dim);
        block_mut(&mut core.opening, ty, G)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(o);
        e.compute_traction(core, ty, G).unwrap();
        block(&core.tractions, ty, G).to_vec()
    }

    /// `(K, J)` at `o`: the tangent (`K[j + dim·i] = ∂T_i/∂Δ_j`) and the
    /// central differences `J[i][j]` of the traction (step `h`).
    fn exp_tangent_and_fd(
        core: &mut CohesiveCore,
        e: &Exponential,
        o: &[Real],
        h: Real,
    ) -> (Vec<Real>, Vec<Vec<Real>>) {
        let dim = core.dim;
        let mut fd = vec![vec![0.0; dim]; dim];
        for j in 0..dim {
            let (mut p, mut m) = (o.to_vec(), o.to_vec());
            p[j] += h;
            m[j] -= h;
            let tp = exp_traction(core, e, &p);
            let tm = exp_traction(core, e, &m);
            for i in 0..dim {
                fd[i][j] = (tp[i] - tm[i]) / (2. * h);
            }
        }
        let _ = exp_traction(core, e, o);
        let mut k = vec![0.0; dim * dim];
        e.compute_tangent_traction(core, exp_ty(dim), G, &mut k)
            .unwrap();
        (k, fd)
    }

    /// Worst error of `K` against `J`, relative to the largest entry of `J`.
    fn exp_error(k: &[Real], fd: &[Vec<Real>]) -> Real {
        let dim = fd.len();
        let scale = fd
            .iter()
            .flatten()
            .fold(0.0_f64, |m, v| m.max(v.abs()))
            .max(1e-300);
        let mut worst = 0.0_f64;
        for i in 0..dim {
            for j in 0..dim {
                worst = worst.max((k[j + dim * i] - fd[i][j]).abs() / scale);
            }
        }
        worst
    }

    /// Opening `t·t̂ + n·Δn` of the point (`t̂` ⊥ `n`; in 3-D a second
    /// tangential component `t2` along `n × t̂`).
    fn exp_opening(n: &[Real], t: Real, dn: Real, t2: Real) -> Vec<Real> {
        if n.len() == 2 {
            vec![t * n[1] + dn * n[0], -t * n[0] + dn * n[1]]
        } else {
            // t̂ = normalize(n₁, −n₀, 0), ŝ = n × t̂
            let l = (n[0] * n[0] + n[1] * n[1]).sqrt();
            let th = [n[1] / l, -n[0] / l, 0.0];
            let sh = [
                n[1] * th[2] - n[2] * th[1],
                n[2] * th[0] - n[0] * th[2],
                n[0] * th[1] - n[1] * th[0],
            ];
            (0..3).map(|i| t * th[i] + t2 * sh[i] + dn * n[i]).collect()
        }
    }

    /// **UD-248**: the default `cohesive_exponential`
    /// tangent is the derivative of its traction on every branch, 2-D and
    /// 3-D, mixed mode (`β = 0.8`): loading, unloading below the committed
    /// `δ_max`, compression with sliding on loading and below `δ_max`.
    /// Upstream's expression (compat) is exact on loading only: it ignores
    /// `δ_max` and keeps a normal stiffness on the compression branch.
    #[test]
    fn ud248_exponential_tangent_is_the_derivative_of_the_traction() {
        // (label, committed δ_max, tangential, normal, second tangential,
        // upstream exact there)
        let cases: [(&str, Real, Real, Real, Real, bool); 6] = [
            ("loading", 0.0, 0.15, 0.2, 0.05, true),
            ("loading past delta_c", 0.3, 0.4, 0.6, 0.1, true),
            ("unloading", 0.6, 0.15, 0.2, 0.05, false),
            ("compression, sliding loading", 0.05, 0.3, -0.05, 0.1, false),
            (
                "compression, sliding below delta_max",
                0.6,
                0.2,
                -0.05,
                0.1,
                false,
            ),
            (
                "compression, sliding past delta_c",
                0.05,
                1.2,
                -0.05,
                0.2,
                false,
            ),
        ];
        for dim in [2, 3] {
            for (label, prev, t, dn, t2, upstream_exact) in cases {
                for exp_penalty in [true, false] {
                    let (mut core, mut e, n) = exponential_point(dim, prev);
                    e.exp_penalty = exp_penalty;
                    e.contact_tangent = if exp_penalty { 1.0 } else { 0.7 };
                    let o = exp_opening(&n, t, dn, if dim == 3 { t2 } else { 0.0 });
                    let (k, fd) = exp_tangent_and_fd(&mut core, &e, &o, 1e-7);
                    let err = exp_error(&k, &fd);
                    assert!(
                        err <= 1e-6,
                        "{dim}-D {label} (exp_penalty {exp_penalty}): tangent {k:?} vs \
                         finite differences {fd:?} (relative error {err:e})"
                    );
                    // trials: the committed maximal opening is unchanged
                    assert_eq!(
                        core.delta_max.previous(exp_ty(dim), G).unwrap().as_slice(),
                        &[prev]
                    );
                    core.compat.exponential_tangent = true;
                    let (kc, _) = exp_tangent_and_fd(&mut core, &e, &o, 1e-7);
                    let err_c = exp_error(&kc, &fd);
                    assert_eq!(
                        err_c <= 1e-6,
                        upstream_exact,
                        "{dim}-D {label} (exp_penalty {exp_penalty}): upstream tangent \
                         relative error {err_c:e}"
                    );
                }
            }
        }
    }

    /// **UD-248**: transition points: at `δ = δ_max`
    /// (the point was committed at this opening) the default tangent is the
    /// loading one, the one-sided derivative in an opening direction; below
    /// it, the unloading secant `A·e σc e^{−δ_max/δc}/δc`. At zero opening
    /// (inside the `|δ| < tol` guard) it is the limit of the loading branch,
    /// the initial stiffness `e σc/δc·A`, as upstream.
    #[test]
    fn ud248_exponential_tangent_at_the_transition_points() {
        for dim in [2, 3] {
            for (t, dn) in [(0.15, 0.2), (0.3, -0.05)] {
                let (mut core, e, n) = exponential_point(dim, 0.0);
                let o = exp_opening(&n, t, dn, if dim == 3 { 0.05 } else { 0.0 });
                // commit δ_max = δ(o) bitwise
                let t0 = exp_traction(&mut core, &e, &o);
                core.delta_max.save_current_values().unwrap();
                let mut k = vec![0.0; dim * dim];
                e.compute_tangent_traction(&core, exp_ty(dim), G, &mut k)
                    .unwrap();
                // direction that opens on both branches: the tangential
                // part plus a quarter of its norm along n (δ grows; a
                // compressed point stays compressed)
                let nn: Real = (0..dim).map(|i| o[i] * n[i]).sum();
                let tg: Vec<Real> = (0..dim).map(|i| o[i] - n[i] * nn).collect();
                let tl = tg.iter().map(|x| x * x).sum::<Real>().sqrt();
                let d: Vec<Real> = (0..dim).map(|i| tg[i] + 0.25 * tl * n[i]).collect();
                let h = 1e-8;
                for (sign, loading) in [(1.0, true), (-1.0, false)] {
                    let p: Vec<Real> = (0..dim).map(|i| o[i] + sign * h * d[i]).collect();
                    let tp = exp_traction(&mut core, &e, &p);
                    let kd: Vec<Real> = (0..dim)
                        .map(|i| (0..dim).map(|j| k[j + dim * i] * d[j]).sum())
                        .collect();
                    let scale = kd.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
                    let err = (0..dim)
                        .map(|i| ((tp[i] - t0[i]) / (sign * h) - kd[i]).abs() / scale)
                        .fold(0.0_f64, f64::max);
                    // one-sided differences: O(h) truncation
                    assert_eq!(
                        err <= 1e-5,
                        loading,
                        "{dim}-D ({t}, {dn}) sign {sign}: one-sided error {err:e}"
                    );
                }
            }
            // zero opening, intact: e σc/δc (β² I + (1 − β²) n⊗n)
            let (mut core, e, n) = exponential_point(dim, 0.0);
            let _ = exp_traction(&mut core, &e, &vec![0.0; dim]);
            let mut k = vec![0.0; dim * dim];
            e.compute_tangent_traction(&core, exp_ty(dim), G, &mut k)
                .unwrap();
            let b2 = 0.64;
            for i in 0..dim {
                for j in 0..dim {
                    let a = if i == j { b2 } else { 0.0 } + (1. - b2) * n[i] * n[j];
                    let expect = a * exp(1.) * 2.0 / 0.5;
                    assert!(
                        (k[j + dim * i] - expect).abs() <= 1e-12 * 4.0 * exp(1.),
                        "{dim}-D zero opening ({i}, {j}): {} vs {expect}",
                        k[j + dim * i]
                    );
                }
            }
        }
    }

    /// UD-244: away from contact the default uncoupled tangent is the
    /// derivative of the traction (normal and shear loading, then shear
    /// unloading); upstream's shear term misses `β²/κ` (compat).
    #[test]
    fn ud244_uncoupled_tangent_is_the_derivative_of_the_traction() {
        for opening in [[0.2, 0.15], [0.05, 0.15]] {
            let (mut core, mut u) = uncoupled_point();
            let (k, fd) = tangent_and_fd(&mut core, &mut u, opening, opening);
            assert!(matches(&k, &fd), "{opening:?}: tangent {k:?} vs {fd:?}");
            core.compat.uncoupled_tangent = true;
            let (kc, _) = tangent_and_fd(&mut core, &mut u, opening, opening);
            assert!(!matches(&kc, &fd), "{opening:?}: compat {kc:?}");
        }
    }

    // ------------------------------------------------------------------
    // cohesive_linear_friction (UD-249, UD-225, UD-247)
    // ------------------------------------------------------------------

    /// Element type of a point of dimension `dim`.
    fn fr_ty(dim: usize) -> ElementType {
        match dim {
            1 => ElementType::Cohesive1d2,
            2 => TY,
            _ => ElementType::Cohesive3d6,
        }
    }

    fn fr_set(f: &mut InternalField<Real>, ty: ElementType, v: &[Real]) {
        let mut a = block_mut(f, ty, G).unwrap();
        a.try_resize(v.len() / a.nb_component(), 0.0).unwrap();
        a.as_mut_slice().copy_from_slice(v);
    }

    /// The unit normal of a friction point of dimension `dim`.
    fn fr_normal(dim: usize) -> Vec<Real> {
        match dim {
            1 => vec![1.0],
            2 => vec![0.6, 0.8],
            _ => {
                let v: [Real; 3] = [0.36, 0.48, 0.8];
                let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                v.iter().map(|x| x / l).collect()
            }
        }
    }

    /// A `cohesive_linear_friction` point of dimension `dim` (normal
    /// [`fr_normal`], `σc,eff = 2`, `δc,eff = 0.5`, `β = 0.8`, `κ = 1.5`,
    /// penalty 50, `μ = 0.3`, `k_f = 40`), implicit (`δ_max` history),
    /// whose committed state is `δ_max = dmax` and `s = s_p`.
    fn friction_point(dim: usize, dmax: Real, s_p: Real) -> (CohesiveCore, Friction, Vec<Real>) {
        let ty = fr_ty(dim);
        let mut core = CohesiveCore::new(dim).unwrap();
        let mut f = Friction::new(&mut core).unwrap();
        core.default_solver_explicit = false;
        core.delta_max.initialize_history();
        f.linear.beta = 0.8;
        f.linear.kappa = 1.5;
        f.linear.penalty = 50.;
        f.linear.update_internal_parameters();
        f.mu_max = 0.3;
        f.friction_penalty = 40.;
        let n = fr_normal(dim);
        let z = vec![0.0; dim];
        fr_set(&mut core.normals, ty, &n);
        for a in [
            &mut core.tractions,
            &mut core.contact_tractions,
            &mut core.contact_opening,
            &mut core.opening,
            &mut f.linear.insertion_stress,
            &mut f.friction_force,
        ] {
            fr_set(a, ty, &z);
        }
        fr_set(&mut f.linear.sigma_c_eff, ty, &[2.0]);
        fr_set(&mut f.linear.delta_c_eff, ty, &[0.5]);
        fr_set(&mut core.delta_max, ty, &[dmax]);
        fr_set(&mut core.damage, ty, &[std_min(dmax / 0.5, 1.)]);
        fr_set(&mut f.residual_sliding, ty, &[s_p]);
        core.delta_max.save_current_values().unwrap();
        core.opening.save_current_values().unwrap();
        f.residual_sliding.save_current_values().unwrap();
        (core, f, n)
    }

    /// Total traction (cohesive + contact) of the friction point at `o` (a
    /// trial against the committed state).
    fn fr_traction(core: &mut CohesiveCore, f: &mut Friction, o: &[Real]) -> Vec<Real> {
        let ty = fr_ty(core.dim);
        block_mut(&mut core.opening, ty, G)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(o);
        let explicit = core.default_solver_explicit;
        f.compute_traction(core, ty, G, explicit).unwrap();
        let t = block(&core.tractions, ty, G);
        let c = block(&core.contact_tractions, ty, G);
        (0..core.dim).map(|i| t[i] + c[i]).collect()
    }

    /// `(K, J)` at `o`: the tangent after a traction evaluation at `o`
    /// (`K[j + dim·i] = ∂T_i/∂Δ_j`) and the central differences `J[i][j]`
    /// (step `h`).
    fn fr_tangent_and_fd(
        core: &mut CohesiveCore,
        f: &mut Friction,
        o: &[Real],
        h: Real,
    ) -> (Vec<Real>, Vec<Vec<Real>>) {
        let dim = core.dim;
        let mut fd = vec![vec![0.0; dim]; dim];
        for j in 0..dim {
            let (mut p, mut m) = (o.to_vec(), o.to_vec());
            p[j] += h;
            m[j] -= h;
            let tp = fr_traction(core, f, &p);
            let tm = fr_traction(core, f, &m);
            for i in 0..dim {
                fd[i][j] = (tp[i] - tm[i]) / (2. * h);
            }
        }
        let _ = fr_traction(core, f, o);
        let mut k = vec![0.0; dim * dim];
        f.compute_tangent_traction(core, fr_ty(dim), G, &mut k)
            .unwrap();
        (k, fd)
    }

    /// Opening `t·t̂ + Δn·n (+ t2·ŝ in 3-D)` with `t̂ ⊥ n` (see
    /// [`exp_opening`]).
    fn fr_opening(n: &[Real], t: Real, dn: Real, t2: Real) -> Vec<Real> {
        if n.len() == 1 {
            vec![dn * n[0]]
        } else {
            exp_opening(n, t, dn, t2)
        }
    }

    /// **UD-247**: the default friction tangent
    /// is the derivative of the traction in contact on every friction
    /// branch — forward stick, forward slip, reverse stick, reverse slip —
    /// below the committed `δ_max` (unloading secant) and on loading, 2-D and
    /// 3-D (in 3-D the tangential opening has two components, so the
    /// direction term `τ/x·(I − n⊗n − t̂⊗t̂)` acts), plus zero tangential
    /// opening with `s_p = 0` (UD-225, where `f = k_f Δt`) and the open
    /// branch. Upstream's tangent (compat) is not: it misses `∂τ_max/∂Δn`
    /// while slipping and decides the stick stiffness from the previous
    /// normal opening (here zero, so it never adds it).
    #[test]
    fn ud247_friction_tangent_is_the_derivative_of_the_traction() {
        // (label, committed δ_max, s_p, tangential, Δn, second tangential,
        // upstream exact there); τ_max = 0.3·50·|Δn| = 3 at Δn = −0.2
        let cases: [(&str, Real, Real, Real, Real, Real, bool); 10] = [
            ("stick", 0.3, 0.1, 0.12, -0.2, 0.02, false),
            ("slip", 0.3, 0.1, 0.25, -0.2, 0.05, false),
            ("reverse stick", 0.3, 0.2, 0.15, -0.2, 0.03, false),
            ("reverse slip", 0.3, 0.3, 0.1, -0.2, 0.02, false),
            ("stick, loading", 0.01, 0.1, 0.12, -0.2, 0.02, false),
            ("slip, loading", 0.01, 0.1, 0.25, -0.2, 0.05, false),
            // Δ = 0.48 < δc (a large opening, not past
            // δc); the next case loads past δc (damage 1): in contact from
            // the converged damage 0.02 by default, out of contact for
            // upstream (the damage 1 of the previous trial), whose zero
            // traction and zero tangent then agree
            (
                "reverse slip, large opening",
                0.01,
                1.1,
                0.9,
                -0.2,
                0.1,
                false,
            ),
            (
                "slip, loading past delta_c",
                0.01,
                0.1,
                1.2,
                -0.2,
                0.1,
                true,
            ),
            (
                "zero tangential opening, s_p = 0",
                0.3,
                0.0,
                0.0,
                -0.2,
                0.0,
                false,
            ),
            ("open, sliding", 0.3, 0.1, 0.12, 0.05, 0.02, true),
        ];
        let mut worst: Real = 0.0;
        for dim in [2, 3] {
            for (label, dmax, s_p, t, dn, t2, upstream_exact) in cases {
                let (mut core, mut f, n) = friction_point(dim, dmax, s_p);
                let o = fr_opening(&n, t, dn, if dim == 3 { t2 } else { 0.0 });
                let (k, fd) = fr_tangent_and_fd(&mut core, &mut f, &o, 1e-7);
                let err = exp_error(&k, &fd);
                worst = worst.max(err);
                eprintln!("{dim}-D {label}: relative error {err:.2e}");
                assert!(
                    err <= 1e-6,
                    "{dim}-D {label}: tangent {k:?} vs finite differences {fd:?} \
                     (relative error {err:e})"
                );
                // trials: the committed state is unchanged
                let ty = fr_ty(dim);
                assert_eq!(
                    f.residual_sliding.previous(ty, G).unwrap().as_slice(),
                    &[s_p]
                );
                assert_eq!(core.delta_max.previous(ty, G).unwrap().as_slice(), &[dmax]);
                core.compat.friction_upstream_implicit = true;
                let (kc, fdc) = fr_tangent_and_fd(&mut core, &mut f, &o, 1e-7);
                let err_c = exp_error(&kc, &fdc);
                eprintln!("{dim}-D {label}: upstream tangent relative error {err_c:.2e}");
                assert_eq!(
                    err_c <= 1e-6,
                    upstream_exact,
                    "{dim}-D {label}: upstream tangent relative error {err_c:e}"
                );
            }
        }
        eprintln!("ud247 friction tangent: worst relative error {worst:.2e}");
    }

    /// **UD-1160** (D-099 (1)) for `cohesive_linear_friction`:
    /// a point failed past δc (committed `δ_max` = 0.8 > δc,eff = 0.5,
    /// damage 1; `s_p` = 0.05), trials below `δ_max`: open (zero traction),
    /// compressed without `contact_after_breaking` (no contact, zero
    /// traction) and in contact after breaking with sticking and slipping
    /// friction (`τ_max` = 3 at Δn = −0.2). By default the tangent is the
    /// derivative of the traction (open: exactly zero), 2-D and 3-D;
    /// upstream's negative secant of the linear part (compat) is not.
    #[test]
    fn ud1160_failed_friction_point_tangent_is_the_derivative_of_the_traction() {
        // (label, tangential, Δn, second tangential, contact after breaking)
        let cases: [(&str, Real, Real, Real, bool); 4] = [
            ("open", 0.12, 0.2, 0.02, false),
            (
                "compressed, no contact after breaking",
                0.12,
                -0.2,
                0.02,
                false,
            ),
            ("contact after breaking, stick", 0.1, -0.2, 0.02, true),
            ("contact after breaking, slip", 0.3, -0.2, 0.05, true),
        ];
        let mut worst: Real = 0.0;
        for dim in [2, 3] {
            for (label, t, dn, t2, cab) in cases {
                let (mut core, mut f, n) = friction_point(dim, 0.8, 0.05);
                f.linear.contact_after_breaking = cab;
                let o = fr_opening(&n, t, dn, if dim == 3 { t2 } else { 0.0 });
                let (k, fd) = fr_tangent_and_fd(&mut core, &mut f, &o, 1e-7);
                let ty = fr_ty(dim);
                assert_eq!(block(&core.damage, ty, G), &[1.0], "{dim}-D {label}");
                let in_contact = block(&core.contact_tractions, ty, G)
                    .iter()
                    .any(|c| *c != 0.0);
                assert_eq!(in_contact, cab, "{dim}-D {label}: contact");
                if !cab {
                    assert!(
                        k.iter().all(|v| *v == 0.0),
                        "{dim}-D {label}: zero tangent ({k:?})"
                    );
                    assert!(
                        fd.iter().flatten().all(|v| *v == 0.0),
                        "{dim}-D {label}: zero traction"
                    );
                }
                let err = exp_error(&k, &fd);
                worst = worst.max(err);
                eprintln!("{dim}-D {label}: relative error {err:.2e}");
                assert!(
                    err <= 1e-6,
                    "{dim}-D {label}: tangent {k:?} vs finite differences {fd:?} \
                     (relative error {err:e})"
                );
                core.compat.tangent_failed_secant = true;
                let (kc, fdc) = fr_tangent_and_fd(&mut core, &mut f, &o, 1e-7);
                let scale = fdc
                    .iter()
                    .flatten()
                    .chain(&kc)
                    .fold(0.0_f64, |m, v| m.max(v.abs()));
                let mut err_c: Real = 0.0;
                for i in 0..dim {
                    for j in 0..dim {
                        err_c = err_c.max((kc[j + dim * i] - fdc[i][j]).abs() / scale);
                    }
                }
                eprintln!("{dim}-D {label}: upstream secant relative error {err_c:.2e}");
                assert!(
                    err_c > 1e-3,
                    "{dim}-D {label}: upstream's secant must differ ({err_c:e})"
                );
            }
        }
        eprintln!("ud1160 friction tangent after failure: worst relative error {worst:.2e}");
    }

    /// **UD-247**: an implicit friction trial depends only on its opening
    /// and the committed state. A compression trial with sliding, then a
    /// trial past complete failure (damage 1), then the compression trial
    /// again: bitwise the same traction, friction force, residual sliding
    /// and damage by default; upstream (compat) decides contact from the
    /// damage the overshooting trial left and loses the contact and the
    /// friction (−2e6 → 0).
    #[test]
    fn ud247_friction_trials_are_idempotent() {
        for dim in [2, 3] {
            for upstream in [false, true] {
                let (mut core, mut f, n) = friction_point(dim, 0.1, 0.05);
                core.compat.friction_upstream_implicit = upstream;
                let ty = fr_ty(dim);
                let a = fr_opening(&n, 0.12, -0.2, if dim == 3 { 0.02 } else { 0.0 });
                let b = fr_opening(&n, 0.0, 0.6, 0.0);
                let snapshot = |core: &CohesiveCore, f: &Friction, t: &[Real]| {
                    let mut v: Vec<u64> = t.iter().map(|x| x.to_bits()).collect();
                    for arr in [
                        block(&f.friction_force, ty, G),
                        block(&f.residual_sliding, ty, G),
                        block(&core.damage, ty, G),
                        block(&core.delta_max, ty, G),
                    ] {
                        v.extend(arr.iter().map(|x| x.to_bits()));
                    }
                    v
                };
                let t1 = fr_traction(&mut core, &mut f, &a);
                let s1 = snapshot(&core, &f, &t1);
                let tb = fr_traction(&mut core, &mut f, &b);
                assert_eq!(block(&core.damage, ty, G), &[1.0], "{dim}-D: overshoot");
                assert!(tb.iter().all(|x| *x == 0.0), "{dim}-D: broken in tension");
                let t2 = fr_traction(&mut core, &mut f, &a);
                let s2 = snapshot(&core, &f, &t2);
                let friction = block(&f.friction_force, ty, G).iter().any(|x| *x != 0.0);
                if upstream {
                    assert_ne!(s1, s2, "{dim}-D: upstream loses the contact");
                    assert!(
                        !friction,
                        "{dim}-D: upstream: no friction after the overshoot"
                    );
                } else {
                    assert_eq!(s1, s2, "{dim}-D: trial A after B");
                    assert!(friction, "{dim}-D: friction in contact");
                }
            }
        }
    }

    /// **UD-225** (A): a penetrating point with zero tangential opening —
    /// always in 1-D, in 2-D/3-D when the tangential opening vanishes — has
    /// a zero friction force (upstream `τ·Δt/‖Δt‖` = 0/0 = NaN); the scalar
    /// residual sliding follows the corrected update (`s = −τ/k_f =
    /// min(s_p, τ_max/k_f)`), the contact traction is the penalty's, and
    /// the tangent is finite.
    #[test]
    fn ud225_zero_tangential_opening_has_no_friction_force() {
        for dim in [1, 2, 3] {
            // s_p = 0.05 (stick: k s_p = 2 < τ_max = 3) and 0.2 (slip)
            for s_p in [0.05, 0.2] {
                let (mut core, mut f, n) = friction_point(dim, 0.3, s_p);
                let ty = fr_ty(dim);
                let o = fr_opening(&n, 0.0, -0.2, 0.0);
                let t = fr_traction(&mut core, &mut f, &o);
                let ff = block(&f.friction_force, ty, G).to_vec();
                assert!(ff.iter().all(|x| *x == 0.0), "{dim}-D s_p {s_p}: {ff:?}");
                let s = block(&f.residual_sliding, ty, G)[0];
                assert_eq!(s, -(-std_min(40. * s_p, 3.0)) / 40., "{dim}-D s_p {s_p}");
                // only the penalty acts (the cohesive part has no
                // tangential opening and drops Δn on penetration)
                for i in 0..dim {
                    assert_eq!(
                        t[i],
                        n[i] * (o.iter().zip(&n).map(|(a, b)| a * b).sum::<Real>()) * 50.
                    );
                }
                let mut k = vec![0.0; dim * dim];
                f.compute_tangent_traction(&mut core, ty, G, &mut k)
                    .unwrap();
                assert!(k.iter().all(|x| x.is_finite()), "{dim}-D s_p {s_p}: {k:?}");
            }
        }
    }

    /// **UD-249** at the law level: a committed stick → slip → reverse
    /// stick → reverse slip → reload cycle of one point at constant
    /// penetration follows the play operator `s_n = clamp(s_{n−1}, x_n − r,
    /// x_n + r)`, `τ_n = k_f (x_n − s_n)`, `r = τ_max/k_f`, by default
    /// (within the closed-form bound 1e-13 τ_max; measured ≤ 3.4ε);
    /// upstream's update (compat) leaves it at the first reverse-stick step
    /// (1.17 τ_max).
    #[test]
    fn ud249_residual_sliding_is_the_play_operator() {
        for dim in [2, 3] {
            for upstream in [false, true] {
                let (mut core, mut f, n) = friction_point(dim, 0.3, 0.0);
                core.compat.friction_residual_sliding_abs = upstream;
                let ty = fr_ty(dim);
                let (k, tau_max) = (40.0, 3.0);
                let r: Real = tau_max / k;
                let mut path = Vec::new();
                let mut ramp = |to: Real, steps: u32| {
                    let from = path.last().copied().unwrap_or(0.0);
                    for i in 1..=steps {
                        path.push(from + (to - from) * Real::from(i) / Real::from(steps));
                    }
                };
                ramp(0.3, 20);
                ramp(0.02, 18);
                ramp(0.18, 10);
                let (mut s_cf, mut worst, mut first) = (0.0_f64, 0.0_f64, None);
                for (step, &x) in path.iter().enumerate() {
                    // a fixed tangential direction of norm x (3-D: two
                    // components, 0.8 x and 0.6 x)
                    let o = if dim == 2 {
                        fr_opening(&n, x, -0.2, 0.0)
                    } else {
                        fr_opening(&n, x * 0.8, -0.2, x * 0.6)
                    };
                    let _ = fr_traction(&mut core, &mut f, &o);
                    let ff = block(&f.friction_force, ty, G).to_vec();
                    let tt: Vec<Real> = (0..dim)
                        .map(|i| o[i] - n[i] * o.iter().zip(&n).map(|(a, b)| a * b).sum::<Real>())
                        .collect();
                    let xn = tt.iter().map(|v| v * v).sum::<Real>().sqrt();
                    let tau = ff.iter().zip(&tt).map(|(a, b)| a * b).sum::<Real>() / xn;
                    s_cf = s_cf.max(xn - r).min(xn + r);
                    let tau_cf = k * (xn - s_cf);
                    let s = block(&f.residual_sliding, ty, G)[0];
                    let e = (tau - tau_cf).abs().max((s - s_cf).abs() * k) / tau_max;
                    // `CLOSED_FORM` (1e-13 of the scale, here τ_max)
                    if e > 1e-13 && first.is_none() {
                        first = Some(step);
                    }
                    worst = worst.max(e);
                    // commit
                    f.residual_sliding.save_current_values().unwrap();
                    core.delta_max.save_current_values().unwrap();
                }
                eprintln!(
                    "{dim}-D upstream {upstream}: worst {worst:.2e} τ_max, first difference {first:?}"
                );
                if upstream {
                    // the first reverse-stick step: after the slip to s =
                    // 0.3 − r = 0.225, x falls below s at step 24 (x =
                    // 0.3 − 5·0.28/18)
                    assert_eq!(first, Some(24), "{dim}-D upstream: {worst:e}");
                    assert!(worst > 0.5, "{dim}-D upstream: {worst:e}");
                } else {
                    assert_eq!(first, None, "{dim}-D default: {worst:e} τ_max");
                }
            }
        }
    }
}
