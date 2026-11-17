// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_plastic/material_drucker_prager.hh, material_drucker_prager.cc,
// material_drucker_prager_inline_impl.hh.

//! `plastic_drucker_prager` (MAT-022): Drucker–Prager plasticity with a
//! closest-point projection (upstream `MaterialDruckerPrager<dim> :
//! MaterialPlastic<dim>`), small strain, all dimensions.
//!
//! Parameters `phi` (friction angle in degrees, 0), `fc` (compressive
//! strength, 1), `radial_return` (true), plus `h`, `sigma_y` of
//! `MaterialPlastic` (unused by the yield function). Derived
//! (`updateInternalParameters`): `φ_r = φπ/180`, `α = 6 sin φ_r/(3 −
//! sin φ_r)`, `c = fc(1 − sin φ_r)/(2 cos φ_r)`, `k = 6c cos φ_r/(3 −
//! sin φ_r)`, with `sin`/`cos` from the port of the oracle's glibc
//! `sincos` (`akantu_core::glibc_libm`, platform independent). Yield
//! function `f(σ) = √(3 J2) + α tr σ − k` with the deviator
//! `σ − I tr σ/dim`.
//!
//! Per quadrature point ([`dp_stress_on_quad`]): trial `σ_tr = σ_prev +
//! MaterialElastic(Δ∇u, σ_th − σ_th,prev)`; if `f(σ_tr) > 0`, the plastic
//! strain increment of the return selected by
//! [`LawOptions::drucker_prager_return`]; final stress `σ_prev +
//! C:(Δ∇u − Δε_p) + Δσ_th` (UD-038). Tangent: the elastic tangent
//! (upstream `MaterialElastic::computeTangentModuli`). Finite
//! deformation: `NotImplemented`, as upstream.
//!
//! * [`DruckerPragerReturn::ClosedForm`] (default, UD-144 to UD-146):
//!   [`dp_return_closed_form`], the exact closest-point projection for the
//!   isotropic elastic operator: the cone return or the apex
//!   `σ = k/(dim·α)·I`, chosen by the closest-point criterion; with
//!   `radial_return` the return at fixed pressure. A yield surface without
//!   admissible stress and a non-finite result are [`Error::Numerical`].
//! * With [`DruckerPragerReturn::UpstreamIteration`], upstream's
//!   `computeGradientAndPlasticMultplier`
//!   ([`dp_closest_point_projection`]): apex branch when `tr σ ≥ k/α` and
//!   `α > 0`; otherwise Newton on the Voigt residual `(σ_tr − σ) − Dₑ∇f Δλ`
//!   and `f`, at most 100 iterations, tolerance 1e-10; with `radial_return`
//!   the iteration uses `α' = 0`, `k' = |α tr σ_tr − k|`; shear components
//!   of the Voigt plastic strain halved. Reproduced bit for bit where it
//!   gives a finite result; non-finite iterates and apex loops beyond 1000
//!   iterations are [`Error::Numerical`] (upstream: NaN or a hang).
//!
//! [`LawOptions::upstream`] reproduces upstream bit for bit.
//!
//! # Evaluation order (D-008)
//!
//! Upstream mixes run-time-sized (`Matrix<Real>`) and compile-time-sized
//! matrices; the port follows each: the trial stress and the yield function
//! of the trial take traces of run-time-sized matrices (sequential sums),
//! everything inside the projection is compile-time-sized (halving trees,
//! coefficient-based products, closed-form or LU inverses of
//! `akantu_core::tensor`).
//!
//! # Upstream defects
//!
//! * **UD-143** (category A, no switch): the Voigt Kronecker vector of the
//!   projection is declared without initialisation and only its normal
//!   components are set (`kronecker_delta.block<dim, 1>(0, 0).fill(1.)`), so
//!   its shear components are stack contents that enter the Hessian (and
//!   the gradient unless `radial_return`). The port uses zeros (the Voigt
//!   form of `δ_ij`) in both modes.
//! * **UD-144** (category B): the apex iterations (`while (tolerance <
//!   projection_error)`) have no iteration limit (upstream can hang) and
//!   invert a singular deviator (NaN plastic strains without an error).
//! * **UD-145** (category B): the trace correction of the apex branch ends
//!   at an interior stress, and the apex is chosen by `tr σ_tr ≥ k/α`
//!   instead of the closest-point criterion.
//! * **UD-146** (category B): the Voigt scaling of the gradient and the
//!   Hessian is inconsistent with the yield function, and the Newton loop
//!   stops after 100 iterations without a convergence check.
//! * **UD-038** (category B): the final stress drops the thermal increment
//!   upstream; [`LawOptions::plastic_thermal_increment`] (default) adds it.
//!
//! [`LawOptions::plastic_thermal_increment`]: crate::material::LawOptions::plastic_thermal_increment
//! [`LawOptions::drucker_prager_return`]: crate::material::LawOptions::drucker_prager_return
//! [`LawOptions::upstream`]: crate::material::LawOptions::upstream

use std::sync::Arc;

use akantu_core::eigen_order as eo;
use akantu_core::enums::MatrixType;
use akantu_core::exec::{Tuples, TuplesRef};
use akantu_core::glibc_libm;
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt, Vector, VectorExt, as_matrix};
use akantu_core::voigt::{matrix_to_voigt, voigt_to_matrix};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};

use super::damage::std_max;
use super::elastic::{ElasticConstants, MaterialElastic};
use super::plastic::{
    PlasticFields, plastic_energy_densities, plastic_potential_energy,
    stress_and_inelastic_strain_on_quad,
};
use crate::material::{
    DruckerPragerReturn, LawOptions, Material, MaterialBase, MaterialFactory, MaterialParams,
};
use crate::material_tensor::{deviatoric, isotropic_stress_coeff, tangent_stiffness_voigt_size};

/// Iteration limit of the apex projections (upstream: none).
const APEX_MAX_ITERATIONS: usize = 1000;

/// Drucker–Prager constants of the projection.
#[derive(Clone, Copy, Debug)]
pub struct DpConstants<const D: usize> {
    /// Elastic kernel constants.
    pub elastic: ElasticConstants<D>,
    /// `α`.
    pub alpha: Real,
    /// `k`.
    pub k: Real,
    /// `radial_return`.
    pub radial_return: bool,
}

/// Trace of a run-time-sized `D × D` matrix (Eigen: sequential sum).
fn trace_dyn<const D: usize>(m: &Matrix<D, D>) -> Real {
    eo::sum(D, eo::READ_COST, false, &|i| m[i][i])
}

/// `MaterialElastic::computeStressOnQuad` on run-time-sized operands
/// (sequential trace).
fn elastic_stress_dyn<const D: usize>(
    c: &ElasticConstants<D>,
    grad_u: &Matrix<D, D>,
    sigma_th: Real,
) -> Matrix<D, D> {
    if D == 1 {
        let mut s = Matrix::<D, D>::zeros();
        s[0][0] = c.e * grad_u[0][0] + sigma_th;
        return s;
    }
    let trace = trace_dyn(grad_u);
    let eps = crate::material_tensor::grad_u_to_epsilon(grad_u);
    let two_mu = 2. * c.mu;
    let diag = c.lambda * trace + sigma_th;
    std::array::from_fn(|j| {
        std::array::from_fn(|i| isotropic_stress_coeff(two_mu, eps[j][i], diag, i, j))
    })
}

/// `computeDeviatoric<dim>(σ)` of a run-time-sized `σ` (sequential trace).
fn deviatoric_dyn<const D: usize>(sigma: &Matrix<D, D>) -> Matrix<D, D> {
    let tr = trace_dyn(sigma);
    #[allow(clippy::cast_precision_loss)] // D <= 3
    let d = D as Real;
    std::array::from_fn(|j| {
        std::array::from_fn(|i| {
            let id = if i == j { 1. } else { 0. };
            sigma[j][i] - id * tr / d
        })
    })
}

/// Upstream `computeYieldFunction(const Matrix<Real> & σ)` (run-time-sized
/// `σ`).
#[must_use]
pub fn dp_yield_function_dyn<const D: usize>(alpha: Real, k: Real, sigma: &Matrix<D, D>) -> Real {
    let sigma_dev = deviatoric_dyn(sigma);
    let j2 = (1. / 2.) * sigma_dev.double_dot(&sigma_dev);
    let sigma_dev_eff = (3. * j2).sqrt();
    let modified_yield_stress = alpha * trace_dyn(sigma) - k;
    sigma_dev_eff + modified_yield_stress
}

/// Result of the closest-point projection.
#[derive(Clone, Copy, Debug)]
pub struct DpProjection<const S: usize> {
    /// Plastic multiplier.
    pub plastic_multiplier: Real,
    /// Gradient of the yield surface (Voigt).
    pub gradient: Vector<S>,
    /// Plastic strain increment (Voigt, engineering shears).
    pub delta_inelastic_strain: Vector<S>,
}

/// Upstream `computeGradientAndPlasticMultplier(σ_trial, …)` (max 100
/// iterations, tolerance 1e-10) for `D`-dimensional stresses with Voigt
/// size `S` (the [`DruckerPragerReturn::UpstreamIteration`] return).
///
/// An invalid `(D, S)` pair does not compile (the Voigt
/// conversions assert it at compile time), e.g.:
///
/// ```compile_fail
/// use akantu_solid::materials::drucker_prager::{DpConstants, dp_closest_point_projection};
/// use akantu_solid::materials::elastic::ElasticConstants;
/// let c = DpConstants::<2> {
///     elastic: ElasticConstants { e: 1., lambda: 1., mu: 1. },
///     alpha: 1.,
///     k: 1.,
///     radial_return: false,
/// };
/// let _ = dp_closest_point_projection::<2, 6>(&c, &[[0.; 6]; 6], &[[1.; 2]; 2]);
/// ```
///
/// # Errors
/// [`Error::Numerical`] if an apex projection does not converge within
/// `APEX_MAX_ITERATIONS`.
#[allow(clippy::too_many_lines)] // one upstream function
pub fn dp_closest_point_projection<const D: usize, const S: usize>(
    c: &DpConstants<D>,
    de: &Matrix<S, S>,
    sigma_trial: &Matrix<D, D>,
) -> Result<DpProjection<S>> {
    const MAX_ITERATIONS: usize = 100;
    const TOLERANCE: Real = 1e-10;
    check_dp_sizes::<D, S>("dp_closest_point_projection")?;
    let (alpha, k) = (c.alpha, c.k);
    let mut sigma_guess = *sigma_trial;
    let mut plastic_multiplier_guess = 0.;
    let gradient_f = Vector::<S>::zeros();
    // δ in Voigt notation; upstream leaves the shear entries uninitialised
    let mut kronecker_delta = Vector::<S>::zeros();
    kronecker_delta[..D].fill(1.);
    let mut scaling_matrix = Matrix::<S, S>::identity();
    for col in scaling_matrix.iter_mut().take(D) {
        for v in col.iter_mut().take(D) {
            *v *= 2.;
        }
    }
    let ce = de.inverse_unchecked();

    if sigma_guess.trace() >= k / alpha && alpha > 0. {
        let first_obj = |sg: &Matrix<D, D>| {
            let sigma_dev = deviatoric(sg);
            (1. / 2.) * sigma_dev.double_dot(&sigma_dev)
        };
        let mut projection_error = first_obj(&sigma_guess);
        let mut it = 0;
        while TOLERANCE < projection_error {
            let sigma_dev = deviatoric(&sigma_guess);
            let inv = sigma_dev.inverse_unchecked();
            for (col, inv_col) in sigma_guess.iter_mut().zip(&inv) {
                for (v, w) in col.iter_mut().zip(inv_col) {
                    *v += -projection_error * w;
                }
            }
            projection_error = first_obj(&sigma_guess);
            if !projection_error.is_finite() {
                // singular deviator (e.g. a vanishing principal deviator):
                // upstream continues with NaN/inf (UD-144)
                return Err(non_finite("apex projection (deviator)"));
            }
            it += 1;
            if it > APEX_MAX_ITERATIONS {
                return Err(Error::Numerical(
                    "plastic_drucker_prager: apex projection (deviator) did not converge".into(),
                ));
            }
        }
        projection_error = alpha * sigma_guess.trace() - k;
        let mut it = 0;
        while TOLERANCE < projection_error {
            for (j, col) in sigma_guess.iter_mut().enumerate() {
                for (i, v) in col.iter_mut().enumerate() {
                    let id = if i == j { 1. } else { 0. };
                    *v += -projection_error * 1. / alpha * id;
                }
            }
            projection_error = alpha * sigma_guess.trace() - k;
            if !projection_error.is_finite() {
                return Err(non_finite("apex projection (trace)"));
            }
            it += 1;
            if it > APEX_MAX_ITERATIONS {
                return Err(Error::Numerical(
                    "plastic_drucker_prager: apex projection (trace) did not converge".into(),
                ));
            }
        }
        let delta_sigma_final = sigma_trial.sub(&sigma_guess);
        let delta_sigma_voigt: Vector<S> = matrix_to_voigt(&delta_sigma_final);
        let delta_inelastic_strain = ce.mul_vec(&delta_sigma_voigt);
        if !delta_inelastic_strain.iter().all(|v| v.is_finite()) {
            return Err(non_finite("apex projection"));
        }
        return Ok(DpProjection {
            plastic_multiplier: plastic_multiplier_guess,
            gradient: gradient_f,
            delta_inelastic_strain,
        });
    }

    let compute_gradient_f = |sg: &Matrix<D, D>, alpha: Real| -> Vector<S> {
        let sigma_dev = deviatoric(sg);
        let sigma_dev_voigt: Vector<S> = matrix_to_voigt(&sigma_dev);
        let j2 = (1. / 2.) * sigma_dev.double_dot(&sigma_dev);
        let p = scaling_matrix.mul_vec(&sigma_dev_voigt);
        let den = 2. * (3. * j2).sqrt();
        std::array::from_fn(|i| p[i] * 3. / den + alpha * kronecker_delta[i])
    };
    let compute_hessian_f = |sg: &Matrix<D, D>| -> Matrix<S, S> {
        let sigma_dev = deviatoric(sg);
        let sigma_dev_voigt: Vector<S> = matrix_to_voigt(&sigma_dev);
        let j2 = (1. / 2.) * sigma_dev.double_dot(&sigma_dev);
        let temp = scaling_matrix.mul_vec(&sigma_dev_voigt);
        let id: Matrix<S, S> = std::array::from_fn(|j| {
            std::array::from_fn(|i| {
                let ident = if i == j { 1. } else { 0. };
                (-kronecker_delta[i]) * kronecker_delta[j] / 3. + ident
            })
        });
        let s2 = 3. / (2. * akantu_core::glibc_libm::pow(3. * j2, 1. / 2.));
        let lhs = scaling_matrix.scale(s2);
        let second = lhs.matmul(&id);
        let den1 = 4. * akantu_core::glibc_libm::pow(3. * j2, 3. / 2.);
        std::array::from_fn(|j| {
            std::array::from_fn(|i| temp[i] * temp[j] * -9. / den1 + second[j][i])
        })
    };
    let sigma_trial_voigt: Vector<S> = matrix_to_voigt(sigma_trial);
    // update_f: gradient, yield function, plastic strain, residual, error
    let update_f =
        |sg: &Matrix<D, D>, pm: Real, k: Real, alpha: Real| -> (Vector<S>, Real, Vector<S>, Real) {
            let gradient = compute_gradient_f(sg, alpha);
            let sigma_dev = deviatoric(sg);
            let j2 = (1. / 2.) * sigma_dev.double_dot(&sigma_dev);
            let sigma_dev_eff = (3. * j2).sqrt();
            let modified_yield_stress = alpha * sg.trace() - k;
            let yield_function = sigma_dev_eff + modified_yield_stress;
            let sigma_guess_voigt: Vector<S> = matrix_to_voigt(sg);
            let tmp = sigma_trial_voigt.sub(&sigma_guess_voigt);
            let dis = ce.mul_vec(&tmp);
            let dg = de.mul_vec(&gradient);
            let f: Vector<S> = std::array::from_fn(|i| tmp[i] - dg[i] * pm);
            let error = std_max(f.norm(), yield_function.abs());
            (gradient, yield_function, dis, error)
        };
    let (alpha_tmp, k_tmp) = if c.radial_return {
        (0., (alpha * sigma_guess.trace() - k).abs())
    } else {
        (alpha, k)
    };
    let (mut gradient_f, mut yield_function, mut delta_inelastic_strain, mut projection_error) =
        update_f(&sigma_guess, plastic_multiplier_guess, k_tmp, alpha_tmp);
    // a NaN error ends upstream's loop at once and returns the current
    // iterate (e.g. the singular gradient of a vanishing deviator: 1-D,
    // negative friction angle, UD-146) — not a converged projection
    if !projection_error.is_finite() {
        return Err(non_finite("projection (singular yield-surface gradient)"));
    }
    let mut iterations = 0;
    while TOLERANCE < projection_error && iterations < MAX_ITERATIONS {
        let hessian_f = compute_hessian_f(&sigma_guess);
        let xi: Matrix<S, S> = std::array::from_fn(|j| {
            std::array::from_fn(|i| ce[j][i] + plastic_multiplier_guess * hessian_f[j][i])
        });
        let xi_inv = xi.inverse_unchecked();
        let tmp = xi_inv.mul_vec(&gradient_f);
        let denominator = gradient_f.dot(&tmp);
        let tmp1 = xi_inv.mul_vec(&delta_inelastic_strain);
        plastic_multiplier_guess = (gradient_f.dot(&tmp1) + yield_function) / denominator;
        let tmp2: Matrix<S, S> = std::array::from_fn(|j| {
            std::array::from_fn(|i| xi_inv[j][i] - tmp[i] * tmp[j] / denominator)
        });
        let t2d = tmp2.mul_vec(&delta_inelastic_strain);
        let delta_sigma: Vector<S> =
            std::array::from_fn(|i| t2d[i] - tmp[i] * yield_function / denominator);
        let delta_sigma_mat: Matrix<D, D> = voigt_to_matrix(&delta_sigma);
        sigma_guess = sigma_guess.add(&delta_sigma_mat);
        let (g, yf, dis, err) = update_f(&sigma_guess, plastic_multiplier_guess, k_tmp, alpha_tmp);
        gradient_f = g;
        yield_function = yf;
        delta_inelastic_strain = dis;
        projection_error = err;
        if !projection_error.is_finite() {
            return Err(non_finite("projection"));
        }
        iterations += 1;
    }
    // upstream stops after MAX_ITERATIONS without a convergence check; the
    // last (finite) iterate is its defined result, kept in compat mode
    Ok(DpProjection {
        plastic_multiplier: plastic_multiplier_guess,
        gradient: gradient_f,
        delta_inelastic_strain,
    })
}

fn non_finite(what: &str) -> Error {
    Error::Numerical(format!("plastic_drucker_prager: non-finite {what}"))
}

/// The public kernels are generic over the dimension `D`
/// and the Voigt size `S`; only `(1, 1)`, `(2, 3)`, `(3, 6)` are valid.
fn check_dp_sizes<const D: usize, const S: usize>(context: &'static str) -> Result<()> {
    if !(1..=3).contains(&D) || S != D * (D + 1) / 2 {
        return Err(Error::DimensionMismatch {
            context,
            expected: D * (D + 1) / 2,
            got: S,
        });
    }
    Ok(())
}

/// The default return of `plastic_drucker_prager` (UD-144…146;
/// [`DruckerPragerReturn::ClosedForm`]): the plastic strain
/// increment `Δε_p` (tensor) of the closest-point projection of the
/// yielding trial `σ_tr` onto `f(σ) = q + α tr σ − k ≤ 0`, `q = √(3J2)`,
/// `J2 = ½ s:s`, `s = σ − (tr σ/D)·I` (the `D`-dimensional deviator of
/// upstream's model), for the isotropic operator `C` of `c.elastic`
/// (`C:s = 2μs` for a deviator, `C:I = (Dλ + 2μ)·I`; `λ` is the plane-stress
/// `λ*` in plane stress; 1-D: `C = E`).
///
/// * Associated (`radial_return = false`): `Δλ = f_tr/(3μ + α²·D(Dλ + 2μ))`;
///   if `q_tr − 3μΔλ ≥ 0` the cone return `Δε_p = Δλ(3s_tr/(2q_tr) + αI)`
///   (the deviator direction is preserved), otherwise the apex.
/// * `radial_return = true` (upstream's `α' = 0`, `k' = |α tr σ_tr − k|`): at
///   fixed pressure, `Δε_p = (q_tr − k')/(3μ)·3s_tr/(2q_tr)` with `k' = k − α
///   tr σ_tr > 0`; if `α tr σ_tr − k ≥ 0` no deviator is admissible at that
///   pressure: the apex.
/// * Apex: `σ = k/(Dα)·I`, `Δε_p = C⁻¹:(σ_tr − σ) = s_tr/(2μ) + (tr σ_tr −
///   k/α)/(D(Dλ + 2μ))·I` (1-D: `(σ_tr − k/α)/E`, the only possible return
///   since `J2 = 0`).
///
/// # Errors
/// [`Error::Numerical`] for an empty elastic domain (apex required with
/// `α = 0`) or a non-finite result.
pub fn dp_return_closed_form<const D: usize>(
    c: &DpConstants<D>,
    sigma_trial: &Matrix<D, D>,
) -> Result<Matrix<D, D>> {
    let (alpha, k) = (c.alpha, c.k);
    let el = &c.elastic;
    #[allow(clippy::cast_precision_loss)] // D <= 3
    let dd = D as Real;
    let i1 = sigma_trial.trace();
    let mut s = *sigma_trial;
    for (i, col) in s.iter_mut().enumerate() {
        col[i] -= i1 / dd;
    }
    let q = (1.5 * s.double_dot(&s)).sqrt();
    // D·(Dλ + 2μ) = tr(C:I); 1-D: E
    let bulk = if D == 1 {
        el.e
    } else {
        dd * (dd * el.lambda + 2. * el.mu)
    };
    let apex = || -> Result<Matrix<D, D>> {
        if alpha == 0. {
            return Err(Error::Numerical(
                "plastic_drucker_prager: no admissible stress (alpha = 0 and k <= 0)".into(),
            ));
        }
        let vol = (i1 - k / alpha) / bulk;
        Ok(std::array::from_fn(|j| {
            std::array::from_fn(|i| {
                let dev = if D == 1 { 0. } else { s[j][i] / (2. * el.mu) };
                dev + if i == j { vol } else { 0. }
            })
        }))
    };
    let depsp = if c.radial_return {
        if alpha * i1 - k >= 0. {
            apex()?
        } else {
            let k_prime = k - alpha * i1;
            let dl = (q - k_prime) / (3. * el.mu);
            s.scale(1.5 * dl / q)
        }
    } else {
        let f = q + alpha * i1 - k;
        let dl = f / (3. * el.mu + alpha * alpha * bulk);
        if D > 1 && q > 0. && q - 3. * el.mu * dl >= 0. {
            std::array::from_fn(|j| {
                std::array::from_fn(|i| dl * (1.5 * s[j][i] / q + if i == j { alpha } else { 0. }))
            })
        } else {
            apex()?
        }
    };
    if !depsp.iter().flatten().all(|v| v.is_finite()) {
        return Err(non_finite("closed-form return"));
    }
    Ok(depsp)
}

/// Upstream `MaterialDruckerPrager::computeStressOnQuad`: returns `(σ, ε_p)`.
/// `de` is the elastic Voigt tangent; `delta_sigma_th = (trial, last)`
/// (upstream: `(σ_th − σ_th,prev, 0)`); `mode` selects the return of a
/// yielding trial ([`dp_return_closed_form`] or upstream's
/// [`dp_closest_point_projection`]). The yield test of the trial and the
/// final stress `σ_prev + C:(Δ∇u − Δε_p)` are upstream's in both modes.
///
/// # Errors
/// Invalid `(D, S)`; see [`dp_return_closed_form`] and
/// [`dp_closest_point_projection`].
#[allow(clippy::too_many_arguments)] // upstream argument tuple
pub fn dp_stress_on_quad<const D: usize, const S: usize>(
    c: &DpConstants<D>,
    de: &Matrix<S, S>,
    grad_u: &Matrix<D, D>,
    previous_grad_u: &Matrix<D, D>,
    previous_sigma: &Matrix<D, D>,
    previous_inelastic_strain: &Matrix<D, D>,
    delta_sigma_th: (Real, Real),
    mode: DruckerPragerReturn,
) -> Result<(Matrix<D, D>, Matrix<D, D>)> {
    check_dp_sizes::<D, S>("dp_stress_on_quad")?;
    let grad_delta_u = grad_u.sub(previous_grad_u);
    let sigma_tr =
        elastic_stress_dyn(&c.elastic, &grad_delta_u, delta_sigma_th.0).add(previous_sigma);
    let initial_yielding = dp_yield_function_dyn(c.alpha, c.k, &sigma_tr) > 0.;
    let mut delta_inelastic_strain = Matrix::<D, D>::zeros();
    if initial_yielding {
        delta_inelastic_strain = match mode {
            DruckerPragerReturn::ClosedForm => dp_return_closed_form(c, &sigma_tr)?,
            DruckerPragerReturn::UpstreamIteration => {
                let p = dp_closest_point_projection::<D, S>(c, de, &sigma_tr)?;
                let mut v = p.delta_inelastic_strain;
                for x in v.iter_mut().skip(D) {
                    *x /= 2.;
                }
                voigt_to_matrix(&v)
            }
        };
    }
    Ok(stress_and_inelastic_strain_on_quad(
        &c.elastic,
        &grad_delta_u,
        &delta_inelastic_strain,
        previous_sigma,
        previous_inelastic_strain,
        delta_sigma_th.1,
    ))
}

/// `plastic_drucker_prager` (upstream `MaterialDruckerPrager<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialDruckerPrager<const DIM: usize> {
    /// The elastic parent.
    pub elastic: MaterialElastic<DIM>,
    /// Plastic internals.
    pub plastic: PlasticFields,
    /// `h` (unused by the yield function).
    pub h: Real,
    /// `sigma_y` (unused by the yield function).
    pub sigma_y: Real,
    /// Friction angle `phi` in degrees.
    pub phi: Real,
    /// Compressive strength `fc`.
    pub fc: Real,
    /// `radial_return`.
    pub radial_return: bool,
    alpha: Real,
    k: Real,
}

impl<const DIM: usize> MaterialDruckerPrager<DIM> {
    /// New law (upstream constructor chain).
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "plastic_drucker_prager".into();
        Ok(Self {
            elastic,
            plastic: PlasticFields::new(DIM)?,
            h: 0.,
            sigma_y: 0.,
            phi: 0.,
            fc: 1.,
            radial_return: true,
            alpha: 0.,
            k: 0.,
        })
    }

    /// Derived `α`.
    #[must_use]
    pub fn alpha(&self) -> Real {
        self.alpha
    }

    /// Derived `k`.
    #[must_use]
    pub fn k(&self) -> Real {
        self.k
    }

    /// Upstream `MaterialDruckerPrager::updateInternalParameters`.
    fn update(&mut self) -> Result<()> {
        MaterialParams::update_params(&mut self.elastic)?;
        let phi_radian = self.phi * std::f64::consts::PI / 180.;
        // The oracle's glibc `sincos` (GCC merges the two
        // calls of upstream), platform independent, not the host libm
        let (s, c) = glibc_libm::sincos_checked(phi_radian).ok_or_else(|| {
            Error::Numerical(format!(
                "plastic_drucker_prager: friction angle phi = {} outside the ported \
                 sin/cos domain (|phi| < 6.04e9 degrees)",
                self.phi
            ))
        })?;
        self.alpha = (6. * s) / (3. - s);
        let cohesion = self.fc * (1. - s) / (2. * c);
        self.k = (6. * cohesion * c) / (3. - s);
        Ok(())
    }

    /// The projection constants.
    #[must_use]
    pub fn dp_constants(&self) -> DpConstants<DIM> {
        DpConstants {
            elastic: self.elastic.constants(),
            alpha: self.alpha,
            k: self.k,
            radial_return: self.radial_return,
        }
    }

    /// The projection over one block (after the thermal refresh). The
    /// points split over `exec`'s threads on copies of the blocks, written
    /// back only when every point succeeded: an error is the first failing
    /// point's and leaves the state unchanged at every split, as the serial
    /// loop.
    fn dp_block(
        &mut self,
        exec: &akantu_core::exec::Exec,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let v = tangent_stiffness_voigt_size(DIM);
        let mut de = vec![0.; v * v];
        self.elastic.tangent_on_quad(&mut de);
        let scalars = DpScalars {
            e: self.elastic.thermal.e,
            lambda: self.elastic.lambda(),
            mu: self.elastic.mu(),
            alpha: self.alpha,
            k: self.k,
            radial_return: self.radial_return,
            corrected_thermal: self.elastic.options.plastic_thermal_increment,
            mode: self.elastic.options.drucker_prager_return,
        };
        let (sigma_th, prev_sigma_th) = {
            let (c, p) = self.elastic.sigma_th.current_and_previous_mut(ty, ghost)?;
            (c.clone(), p.clone())
        };
        let base = self.elastic.base();
        let grad_u = base.grad_u.get(ty, ghost)?.clone();
        let prev_grad_u = base.grad_u.previous(ty, ghost)?.clone();
        let prev_sigma = base.stress.previous(ty, ghost)?.clone();
        let mut sigma = base.sigma_field()?.get(ty, ghost)?.clone();
        let (mut inel, prev_inel) = {
            let (c, p) = self
                .plastic
                .inelastic_strain
                .current_and_previous_mut(ty, ghost)?;
            (c.clone(), p.clone())
        };
        let nq = grad_u.len();
        if [
            prev_grad_u.len(),
            prev_sigma.len(),
            sigma.len(),
            inel.len(),
            prev_inel.len(),
            sigma_th.len(),
            prev_sigma_th.len(),
        ]
        .iter()
        .any(|&l| l != nq)
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialDruckerPrager::computeStress",
                expected: nq,
                got: sigma.len().min(inel.len()),
            });
        }
        let n = DIM * DIM;
        if [
            &grad_u,
            &prev_grad_u,
            &prev_sigma,
            &sigma,
            &inel,
            &prev_inel,
        ]
        .iter()
        .any(|a| a.nb_component() != n)
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialDruckerPrager::computeStress: components",
                expected: n,
                got: sigma.nb_component(),
            });
        }
        let items = (
            (
                TuplesRef::new(grad_u.as_slice(), n),
                TuplesRef::new(prev_grad_u.as_slice(), n),
                TuplesRef::new(prev_sigma.as_slice(), n),
                TuplesRef::new(prev_inel.as_slice(), n),
            ),
            TuplesRef::new(sigma_th.as_slice(), 1),
            TuplesRef::new(prev_sigma_th.as_slice(), 1),
            Tuples::new(sigma.as_mut_slice(), n),
            Tuples::new(inel.as_mut_slice(), n),
        );
        exec.try_for_each(
            items,
            512,
            |_, ((g, pg, ps, pe), th, pth, mut sigma, mut inel)| {
                let data = DpBlock {
                    de: &de,
                    grad_u: g.as_slice(),
                    previous_grad_u: pg.as_slice(),
                    previous_sigma: ps.as_slice(),
                    previous_inelastic_strain: pe.as_slice(),
                    sigma_th: th.as_slice(),
                    previous_sigma_th: pth.as_slice(),
                };
                let (sigma, inel) = (sigma.as_mut_slice(), inel.as_mut_slice());
                match DIM {
                    1 => dp_block_kernel::<1, 1>(&scalars, &data, sigma, inel),
                    2 => dp_block_kernel::<2, 3>(&scalars, &data, sigma, inel),
                    _ => dp_block_kernel::<3, 6>(&scalars, &data, sigma, inel),
                }
            },
        )?;
        self.elastic
            .base_mut()
            .sigma_field_mut()?
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        self.plastic
            .inelastic_strain
            .get_mut(ty, ghost)?
            .copy_from(&inel)?;
        Ok(())
    }
}

/// Scalar data of one block evaluation.
struct DpScalars {
    e: Real,
    lambda: Real,
    mu: Real,
    alpha: Real,
    k: Real,
    radial_return: bool,
    corrected_thermal: bool,
    mode: DruckerPragerReturn,
}

/// Flat per-point data of one block (column-major tuples).
struct DpBlock<'a> {
    de: &'a [Real],
    grad_u: &'a [Real],
    previous_grad_u: &'a [Real],
    previous_sigma: &'a [Real],
    previous_inelastic_strain: &'a [Real],
    sigma_th: &'a [Real],
    previous_sigma_th: &'a [Real],
}

/// [`dp_stress_on_quad`] over the points of a block (`D`, Voigt size `S`).
fn dp_block_kernel<const D: usize, const S: usize>(
    sc: &DpScalars,
    b: &DpBlock<'_>,
    sigma_out: &mut [Real],
    inel_out: &mut [Real],
) -> Result<()> {
    let c = DpConstants::<D> {
        elastic: ElasticConstants {
            e: sc.e,
            lambda: sc.lambda,
            mu: sc.mu,
        },
        alpha: sc.alpha,
        k: sc.k,
        radial_return: sc.radial_return,
    };
    let de: &Matrix<S, S> = as_matrix(b.de)?;
    let n = D * D;
    for q in 0..b.sigma_th.len() {
        let r = q * n..(q + 1) * n;
        let d = b.sigma_th[q] - b.previous_sigma_th[q];
        let th = (d, if sc.corrected_thermal { d } else { 0. });
        let (s, e) = dp_stress_on_quad::<D, S>(
            &c,
            de,
            as_matrix(&b.grad_u[r.clone()])?,
            as_matrix(&b.previous_grad_u[r.clone()])?,
            as_matrix(&b.previous_sigma[r.clone()])?,
            as_matrix(&b.previous_inelastic_strain[r.clone()])?,
            th,
            sc.mode,
        )?;
        sigma_out[r.clone()].copy_from_slice(s.as_flattened());
        inel_out[r].copy_from_slice(e.as_flattened());
    }
    Ok(())
}

impl<const DIM: usize> MaterialParams for MaterialDruckerPrager<DIM> {
    fn params_base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        if DIM == 2 {
            Self::PARAMS_2D
        } else {
            Self::PARAMS
        }
    }
    fn update_params(&mut self) -> Result<()> {
        self.update()
    }
}

impl<const DIM: usize> MaterialDruckerPrager<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(
        Self,
        [elastic],
        [
            crate::real_param!(Self, "h", ParameterAccess::PARSMOD, "Hardening  modulus", h),
            crate::real_param!(
                Self,
                "sigma_y",
                ParameterAccess::PARSMOD,
                "Yield stress",
                sigma_y
            ),
            crate::real_param!(
                Self,
                "phi",
                ParameterAccess::PARSMOD,
                "Internal friction angle in degrees",
                phi
            ),
            crate::real_param!(
                Self,
                "fc",
                ParameterAccess::PARSMOD,
                "Compressive strength",
                fc
            ),
            crate::bool_param!(
                Self,
                "radial_return",
                ParameterAccess::PARSMOD,
                "Radial return mapping",
                radial_return
            ),
        ]
    );
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(plane_stress Self, [elastic], [
        crate::real_param!(Self, "h", ParameterAccess::PARSMOD, "Hardening  modulus", h),
        crate::real_param!(Self, "sigma_y", ParameterAccess::PARSMOD, "Yield stress", sigma_y),
        crate::real_param!(Self, "phi", ParameterAccess::PARSMOD, "Internal friction angle in degrees", phi),
        crate::real_param!(Self, "fc", ParameterAccess::PARSMOD, "Compressive strength", fc),
        crate::bool_param!(Self, "radial_return", ParameterAccess::PARSMOD, "Radial return mapping", radial_return),
    ]);
}

crate::impl_material_registry!(MaterialDruckerPrager<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialDruckerPrager<DIM> {
    fn name(&self) -> &str {
        self.elastic.name()
    }
    fn law_id(&self) -> &str {
        self.elastic.law_id()
    }
    fn law_type(&self) -> &str {
        self.elastic.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.elastic.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.elastic.element_filter_mut()
    }
    /// The base's `(dimension, kind)` restriction for the law's own
    /// internals too (upstream `registerInternal` / `InternalField` with the
    /// law's dimension and `_ek_regular`; row CL-002).
    fn internal_restriction(&self) -> Option<akantu_model::internal_field::TypeRestriction> {
        Some(Material::base(self).internal_restriction())
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.elastic.for_each_internal(f);
        self.plastic.for_each(f);
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        self.plastic.for_each_mut(f)
    }
}

impl<const DIM: usize> Material for MaterialDruckerPrager<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `MaterialElastic::initMaterial`.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.elastic.base_mut().init_history()?;
        let blocks = self.elastic.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.elastic.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        self.update()
    }

    /// Upstream `MaterialDruckerPrager::computeStress`: `NotImplemented`
    /// with finite deformation, thermal refresh, then the projection per
    /// point.
    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        if self.elastic.base().finite_deformation {
            return Err(Error::NotImplemented(
                "computeStress : not implemented yet !",
            ));
        }
        let exec = crate::material::points_exec(self, ctx);
        self.elastic.compute_thermal_with(exec, ty, ghost)?;
        self.dp_block(exec, ty, ghost)
    }

    /// The projection of a point reads only that point's gradients,
    /// stresses, inelastic strains and thermal stresses (copies of the
    /// blocks, written back when every point succeeded) and the law's
    /// constants; the plastic work is per point too.
    fn independent_points(&self) -> bool {
        true
    }

    /// Upstream `MaterialElastic::computeTangentModuli` (not overridden).
    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let _ = tangent_stiffness_voigt_size(DIM);
        self.elastic.compute_tangent_moduli(ctx, ty, ghost, tangent)
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        plastic_potential_energy(&mut self.elastic, &self.plastic.inelastic_strain, None, ty)
    }

    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        match energy_id {
            "plastic" => self.plastic.plastic_energy(self.elastic.base(), ctx),
            "potential" => self.potential_energy(ctx),
            _ => Ok(0.0),
        }
    }

    /// The law's own density (see `MaterialLinearIsotropicHardening`).
    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let points = super::damage::element_points(self.elastic.base(), ctx, element)?;
        plastic_energy_densities(
            &self.elastic,
            &self.plastic.inelastic_strain,
            None,
            element.ty(),
            points,
        )
    }

    fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let exec = crate::material::points_exec(self, ctx);
        self.plastic
            .update_energies::<DIM>(exec, self.elastic.base(), ty)
    }

    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.push_wave_speed(element)
    }

    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.shear_wave_speed(element)
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn has_stiffness_matrix_changed(&self) -> bool {
        self.elastic.has_stiffness_matrix_changed()
    }

    fn mark_stiffness_changed(&mut self) {
        self.elastic.mark_stiffness_changed();
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.elastic.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.elastic.options)
    }

    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        self.elastic.compute_all_cauchy_stresses(ghost)
    }
}

/// Register `plastic_drucker_prager` (upstream
/// `instantiateMaterial<MaterialDruckerPrager>`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "plastic_drucker_prager",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialDruckerPrager::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}
