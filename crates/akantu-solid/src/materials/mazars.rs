// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_damage/material_mazars.hh, material_mazars.cc,
// material_mazars_inline_impl.hh, material_von_mises_mazars.hh,
// material_von_mises_mazars.cc.

//! `mazars` (MAT-013) and `plastic_mazars` (MAT-015): Mazars' isotropic
//! damage (upstream `MaterialMazars<dim, Parent>`) on an elastic parent or
//! on the J2 linear-hardening parent (`MaterialVonMisesMazars<dim> =
//! MaterialMazars<dim, MaterialLinearIsotropicHardening>`).
//!
//! Per quadrature point (`computeStressOnQuad`, operand order kept, D-008),
//! with the default [`LawOptions`]:
//!
//! 1. thermal pre-step of the parent (UD-038), then `σ =
//!    Parent::computeStressOnQuad` (elastic kernel, or the J2 return
//!    mapping of `plastic.rs`, which for `plastic_mazars` starts from the
//!    undamaged stress history `effective_stress`, UD-139);
//! 2. plastic parent: `∇u −= ε_p` on the `grad_u` internal (restored by
//!    `∇u += ε_p` at the end: the stored gradient is `(∇u − ε_p) + ε_p`,
//!    bit for bit as upstream, UD-150);
//! 3. principal strains `ε_i` of the `3 × 3` strain [`mazars_strain_3d`]:
//!    the mechanical strain (the free thermal strain `αΔT` removed, UD-140)
//!    embedded with the out-of-plane strain of the represented 3-D state
//!    (plane stress `ε33 = −ν/(1 − ν)(ε11 + ε22)`, plane strain with the J2
//!    parent `ε_p33 = −tr ε_p`, UD-141); Eigen's `EigenSolver` on the
//!    compile-time `3 × 3` matrix, unsorted real parts
//!    ([`akantu_linalg::dense::eig`], `Storage::Fixed`);
//! 4. `ε̂ = √(Σ ⟨ε_i⟩₊²)` (sequential from 0; stored in the `Ehat`
//!    internal, which upstream keeps as a scalar member, UD-067);
//! 5. local damage ([`mazars_damage_on_quad`]): if `ε̂ − K0 > 0`,
//!    `d_t = 1 − K0(1 − At)/ε̂ − At·exp(−Bt(ε̂ − K0))`, `d_c` likewise with
//!    `Ac, Bc`; principal effective stresses `σ̃_i = C_d ε_i + λ(ε_j + ε_k)`,
//!    `C_d = E(1 − ν)/((1 + ν)(1 − 2ν))` (λ: the 3-D `λ`, also in plane
//!    stress, UD-141); `α_t = min(Σ ε_t,i ⟨ε_i⟩₊ / ε̂², 1)`,
//!    `α_c = 1 − α_t`, both to the power `β` (glibc `exp`/`pow` ports);
//!    `d = std::min(std::max(α_t d_t + α_c d_c, d), 1)` with the damage `d`
//!    committed at the last `savePreviousState` (UD-138);
//! 6. `σ *= 1 − d`.
//!
//! `K0` is a random internal field. Tangent, energies, history:
//! [`super::damage`] (for `plastic_mazars` the plastic work is updated
//! first, with the damaged stress, and the potential energy is
//! `MaterialPlastic`'s `½ σ:(∇u − ε_p)`). With `finite_deformation` both
//! laws return [`Error::Unsupported`] (UD-030, UD-142).
//!
//! [`LawOptions::upstream`] reproduces upstream bit for bit: no thermal
//! refresh, the total strain minus the plastic strain embedded with
//! `ε33 = 0`, the plane-stress `λ*` in plane stress, the maximum taken with
//! the current (trial) damage, the J2 return of `plastic_mazars` started
//! from the damaged stress.
//!
//! # Upstream defects and their switches
//!
//! * **UD-038** (category B): the thermal stress is never refreshed
//!   upstream; [`LawOptions::damage_thermal_refresh`] (default) refreshes it
//!   (for `plastic_mazars` the J2 kernel then sees the thermal increment,
//!   with the sign of [`LawOptions::plastic_thermal_increment`]).
//! * **UD-138** (category B): upstream takes the maximum with the current
//!   damage, so a high trial of a step keeps its damage for the later
//!   trials; [`LawOptions::committed_trial_state`].
//! * **UD-139** (category B): upstream's J2 return of `plastic_mazars`
//!   starts from the damaged stress `(1 − d)·σ_J2`, so the stress decays at
//!   fixed strain; [`LawOptions::plastic_mazars_effective_stress`].
//! * **UD-140** (category B): upstream's equivalent strain uses the total
//!   strain, so free heating damages; [`LawOptions::mazars_mechanical_strain`].
//! * **UD-141** (category B): upstream embeds the 2-D strain with
//!   `ε33 = 0`, plane stress included, and uses the plane-stress `λ*`;
//!   [`LawOptions::mazars_out_of_plane_strain`].
//! * **UD-030** (category B): `plastic_mazars` with `finite_deformation`
//!   runs the small-strain J2 kernel on the raw `∇u` and stores a "PK2"
//!   upstream; [`FiniteStrainPlasticity::UpstreamLegacy`] reproduces it.
//! * **UD-142** (category B): `mazars` and `plastic_mazars` with
//!   `finite_deformation` feed the raw `∇u` to the elastic kernel upstream
//!   (not frame indifferent); [`FiniteStrainDamage::UpstreamLegacy`]
//!   reproduces it.
//!
//! [`LawOptions::damage_thermal_refresh`]: crate::material::LawOptions::damage_thermal_refresh
//! [`LawOptions::plastic_thermal_increment`]: crate::material::LawOptions::plastic_thermal_increment
//! [`LawOptions::committed_trial_state`]: crate::material::LawOptions::committed_trial_state
//! [`LawOptions::plastic_mazars_effective_stress`]: crate::material::LawOptions::plastic_mazars_effective_stress
//! [`LawOptions::mazars_mechanical_strain`]: crate::material::LawOptions::mazars_mechanical_strain
//! [`LawOptions::mazars_out_of_plane_strain`]: crate::material::LawOptions::mazars_out_of_plane_strain
//! [`LawOptions::upstream`]: crate::material::LawOptions::upstream

use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::exec::{Exec, Tuples};
use akantu_core::glibc_libm;
use akantu_core::random::RandomParameter;
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_linalg::dense::eig3_fixed;
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::Parsable;
use akantu_parser::param::{ParamDescriptor, ParameterAccess};

use super::damage::{DamageFields, DamageParent, std_max, std_min};
use super::elastic::MaterialElastic;
use super::plastic::MaterialLinearIsotropicHardening;
use crate::material::{
    FiniteStrainDamage, FiniteStrainPlasticity, LawOptions, Material, MaterialBase,
    MaterialFactory, MaterialParams,
};
use crate::material_tensor::{grad_u_to_epsilon, tangent_stiffness_voigt_size};

/// Mazars parameters (upstream members, defaults of `registerParam`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MazarsParams {
    /// Traction parameter `At`.
    pub at: Real,
    /// Traction parameter `Bt`.
    pub bt: Real,
    /// Compression parameter `Ac`.
    pub ac: Real,
    /// Compression parameter `Bc`.
    pub bc: Real,
    /// Shear parameter `beta`.
    pub beta: Real,
}

impl Default for MazarsParams {
    fn default() -> Self {
        Self {
            at: 0.8,
            bt: 12000.,
            ac: 1.4,
            bc: 1900.,
            beta: 1.06,
        }
    }
}

/// Elastic constants read by the Mazars damage update (`E`, `ν`, `λ`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MazarsElastic {
    /// Young's modulus.
    pub e: Real,
    /// Poisson's ratio.
    pub nu: Real,
    /// First Lamé coefficient (the 3-D value by default; the plane-stress
    /// `λ*` in 2-D plane stress with `mazars_out_of_plane_strain = false`).
    pub lambda: Real,
}

/// Steps 3–4: principal strains of the embedded `3 × 3` strain of
/// `grad_u` (upstream: zero out-of-plane strain; Eigen order) and `ε̂`.
///
/// # Errors
/// [`Error::Numerical`] if the eigen-solver does not converge (upstream
/// would read uninitialised values).
pub fn principal_strains_and_ehat<const D: usize>(
    grad_u: &Matrix<D, D>,
) -> Result<([Real; 3], Real)> {
    let eps = grad_u_to_epsilon(grad_u);
    let mut e3 = Matrix::<3, 3>::zeros();
    for (j, col) in eps.iter().enumerate() {
        e3[j][..D].copy_from_slice(col);
    }
    principal_strains_and_ehat3(&e3)
}

/// Steps 3–4 on a `3 × 3` strain (column-major): Eigen's compile-time
/// `EigenSolver` values ([`eig3_fixed`], no heap allocation)
/// and `ε̂ = √(Σ⟨ε_i⟩₊²)`.
///
/// # Errors
/// [`Error::Numerical`] if the eigen-solver does not converge.
pub fn principal_strains_and_ehat3(e3: &Matrix<3, 3>) -> Result<([Real; 3], Real)> {
    let fdiag = eig3_fixed(e3)?;
    let mut ehat = 0.;
    for &e in &fdiag {
        let ep = std_max(0., e);
        ehat += ep * ep;
    }
    Ok((fdiag, ehat.sqrt()))
}

/// How the `3 × 3` strain driving Mazars' damage is built from a `D`-dim
/// state (UD-140, UD-141; [`LawOptions::mazars_mechanical_strain`],
/// [`LawOptions::mazars_out_of_plane_strain`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MazarsStrainMode {
    /// 2-D plane stress.
    pub plane_stress: bool,
    /// Poisson's ratio (plane-stress out-of-plane strain).
    pub nu: Real,
    /// Subtract the free thermal strain `αΔT` (UD-140).
    pub mechanical: bool,
    /// Embed the out-of-plane strain of the represented 3-D state (UD-141).
    pub out_of_plane: bool,
}

/// The `3 × 3` strain driving Mazars' damage: `ε = sym(g)` with `g = ∇u −
/// ε_p` (the elastic-plus-thermal gradient), minus `alpha_delta_t = αΔT` on
/// the in-plane/axial diagonal when `mode.mechanical`, embedded with the
/// out-of-plane strain of the represented 3-D state when
/// `mode.out_of_plane` (2-D plane stress: `ε33 = −ν/(1 − ν)(ε11 + ε22)`, the
/// elastic strain with `σ33 = 0`; plane strain: `ε33,e = −ε_p33 − αΔT` with
/// `ε_p33 = −inelastic_trace`, isochoric J2 flow); 1-D (uniaxial stress,
/// `ν = 0`) and the mechanical `−αΔT` of plane strain need no
/// `out_of_plane`. With both switches off this is upstream's embedding
/// (zeros), bit for bit; without a temperature change and plastic strain the
/// defaults differ from upstream only in plane stress.
#[must_use]
pub fn mazars_strain_3d<const D: usize>(
    g: &Matrix<D, D>,
    inelastic_trace: Real,
    alpha_delta_t: Real,
    mode: MazarsStrainMode,
) -> Matrix<3, 3> {
    let mut eps = grad_u_to_epsilon(g);
    if mode.mechanical {
        for (i, col) in eps.iter_mut().enumerate() {
            col[i] -= alpha_delta_t;
        }
    }
    let mut e3 = Matrix::<3, 3>::zeros();
    for (j, col) in eps.iter().enumerate() {
        e3[j][..D].copy_from_slice(col);
    }
    if D == 2 {
        if mode.plane_stress {
            if mode.out_of_plane {
                e3[2][2] = -mode.nu / (1. - mode.nu) * (eps[0][0] + eps[1][1]);
            }
        } else {
            let mut e33 = 0.;
            if mode.out_of_plane {
                e33 += inelastic_trace;
            }
            if mode.mechanical {
                e33 -= alpha_delta_t;
            }
            e3[2][2] = e33;
        }
    }
    e3
}

/// Upstream `MaterialMazars::computeDamageOnQuad`: the new damage from
/// `ε̂`, `K0`, the principal strains and the current damage `dam`.
#[must_use]
pub fn mazars_damage_on_quad(
    p: &MazarsParams,
    el: &MazarsElastic,
    ehat: Real,
    k0: Real,
    epsilon_princ: &[Real; 3],
    dam: Real,
) -> Real {
    let fs = ehat - k0;
    if fs <= 0. {
        return dam;
    }
    let dam_t = 1. - k0 * (1. - p.at) / ehat - p.at * glibc_libm::exp(-p.bt * (ehat - k0));
    let dam_c = 1. - k0 * (1. - p.ac) / ehat - p.ac * glibc_libm::exp(-p.bc * (ehat - k0));
    let (e, nu, lambda) = (el.e, el.nu, el.lambda);
    let cdiag = e * (1. - nu) / ((1. + nu) * (1. - 2. * nu));
    let ep = epsilon_princ;
    let sigma_princ = [
        cdiag * ep[0] + lambda * (ep[1] + ep[2]),
        cdiag * ep[1] + lambda * (ep[0] + ep[2]),
        cdiag * ep[2] + lambda * (ep[1] + ep[0]),
    ];
    let sigma_p = sigma_princ.map(|s| std_max(0., s));
    let trace_p = nu / e * (sigma_p[0] + sigma_p[1] + sigma_p[2]);
    let mut alpha_t = 0.;
    for i in 0..3 {
        let epsilon_t = (1. + nu) / e * sigma_p[i] - trace_p;
        let epsilon_p = std_max(0., ep[i]);
        alpha_t += epsilon_t * epsilon_p;
    }
    alpha_t /= ehat * ehat;
    alpha_t = std_min(alpha_t, 1.);
    let alpha_c = 1. - alpha_t;
    let alpha_t = glibc_libm::pow(alpha_t, p.beta);
    let alpha_c = glibc_libm::pow(alpha_c, p.beta);
    let damtemp = alpha_t * dam_t + alpha_c * dam_c;
    std_min(std_max(damtemp, dam), 1.)
}

/// Mazars damage on the parent `P` (upstream `MaterialMazars<DIM, Parent>`).
#[derive(Clone, Debug)]
pub struct MaterialMazars<const DIM: usize, P> {
    /// The parent law (elastic or J2), owned; only its point kernels run.
    pub parent: P,
    /// `damage`, `damage dissipated energy`, `integral of sigma`.
    pub damage: DamageFields,
    /// Damage threshold `K0` (random internal field).
    pub k0: InternalField<Real>,
    /// Equivalent strain `ε̂` of the last stress evaluation per point
    /// (`Ehat`; upstream: a scalar member, UD-067).
    pub ehat: InternalField<Real>,
    /// `At`, `Bt`, `Ac`, `Bc`, `beta`.
    pub params: MazarsParams,
    /// Upstream `damage_in_compute_stress` (`true` for the local laws).
    pub damage_in_compute_stress: bool,
    /// Undamaged (effective) stress of the plastic parent (`dim²`, history;
    /// plastic parents only, internal `effective_stress`): the stress
    /// history of the J2 return by default (UD-139).
    pub effective_stress: Option<InternalField<Real>>,
}

/// `mazars`: Mazars damage on `elastic`.
pub type MaterialMazarsElastic<const DIM: usize> = MaterialMazars<DIM, MaterialElastic<DIM>>;
/// `plastic_mazars`: Mazars damage on `plastic_linear_isotropic_hardening`
/// (upstream `MaterialVonMisesMazars<dim>`).
pub type MaterialVonMisesMazars<const DIM: usize> =
    MaterialMazars<DIM, MaterialLinearIsotropicHardening<DIM>>;

impl<const DIM: usize> MaterialMazarsElastic<DIM> {
    /// New `mazars` law (upstream constructor chain).
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        Self::with_parent(MaterialElastic::new(spec, mesh)?, "mazars")
    }
}

impl<const DIM: usize> MaterialVonMisesMazars<DIM> {
    /// New `plastic_mazars` law (upstream constructor chain).
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        Self::with_parent(
            MaterialLinearIsotropicHardening::new(spec, mesh)?,
            "plastic_mazars",
        )
    }
}

impl<const DIM: usize, P: DamageParent<DIM>> MaterialMazars<DIM, P> {
    fn with_parent(mut parent: P, law_type: &str) -> Result<Self> {
        parent.base_mut().law_type = law_type.into();
        let effective_stress = if parent.inelastic_strain().is_some() {
            let mut f = InternalField::new(EFFECTIVE_STRESS, DIM * DIM, 0.0)?;
            f.initialize_history();
            Some(f)
        } else {
            None
        };
        Ok(Self {
            parent,
            damage: DamageFields::new()?,
            k0: InternalField::new_random("K0", 1, RandomParameter::default())?,
            ehat: InternalField::new("Ehat", 1, 0.0)?,
            params: MazarsParams::default(),
            damage_in_compute_stress: true,
            effective_stress,
        })
    }

    /// The elastic constants of the principal weights: upstream's (current
    /// `λ`, the plane-stress `λ*` in plane stress) or, with the consistent
    /// 3-D embedding of plane stress (UD-141), the 3-D `λ`.
    pub(crate) fn elastic_constants(&self) -> MazarsElastic {
        let el = self.parent.elastic();
        let (e, nu) = (el.thermal.e, el.thermal.nu);
        let lambda = if DIM == 2 && el.plane_stress && el.options.mazars_out_of_plane_strain {
            nu * e / ((1. + nu) * (1. - 2. * nu))
        } else {
            el.lambda()
        };
        MazarsElastic { e, nu, lambda }
    }

    pub(crate) fn strain_mode(&self) -> MazarsStrainMode {
        let el = self.parent.elastic();
        MazarsStrainMode {
            plane_stress: el.plane_stress,
            nu: el.thermal.nu,
            mechanical: el.options.mazars_mechanical_strain,
            out_of_plane: el.options.mazars_out_of_plane_strain,
        }
    }

    fn options_ref(&self) -> &LawOptions {
        &self.parent.elastic().options
    }

    /// Steps 2–6 of the module docs over one block (after the parent's
    /// kernel). The points split over `exec`'s threads; they work on copies
    /// of the blocks, written back only when every point succeeded, so a
    /// failed eigen solve leaves the law's state as the serial loop leaves
    /// it (unchanged) whatever the split, and the error is that of the
    /// first failing point.
    #[allow(clippy::too_many_lines)] // one upstream loop with its options
    fn damage_block(&mut self, exec: &Exec, ty: ElementType, ghost: GhostType) -> Result<()> {
        let n = DIM * DIM;
        let el = self.elastic_constants();
        let mode = self.strain_mode();
        let params = self.params;
        let non_local = self.parent.base().is_non_local;
        let damage_in_cs = self.damage_in_compute_stress;
        let committed = self.options_ref().committed_trial_state;
        let alpha = self.parent.elastic().thermal.alpha;
        // the copies of the blocks the points work on, made in parts of the
        // executor's threads (the same values)
        let delta_t = clone_with(exec, &*self.parent.elastic().delta_t.get(ty, ghost)?)?;
        let inel = self
            .parent
            .inelastic_strain()
            .map(|f| f.get(ty, ghost).and_then(|a| clone_with(exec, &a)))
            .transpose()?;
        let k0 = clone_with(exec, &*self.k0.get(ty, ghost)?)?;
        // the damage's deferred save, if any, in parts too
        drop(self.damage.damage.get_mut_with(exec, ty, ghost)?);
        let (mut dam, prev_dam) = {
            let (c, p) = self.damage.damage.current_and_previous_mut(ty, ghost)?;
            (clone_with(exec, &c)?, clone_with(exec, p)?)
        };
        let mut ehat = clone_with(exec, &*self.ehat.get(ty, ghost)?)?;
        let base = self.parent.base();
        let mut grad_u = clone_with(exec, &*base.grad_u.get(ty, ghost)?)?;
        let mut sigma = clone_with(exec, &*base.sigma_field()?.get(ty, ghost)?)?;
        let nq = grad_u.len();
        if [
            sigma.len(),
            k0.len(),
            dam.len(),
            prev_dam.len(),
            ehat.len(),
            delta_t.len(),
        ]
        .iter()
        .any(|&l| l != nq)
            || inel.as_ref().is_some_and(|a| a.len() != nq)
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialMazars::computeStress",
                expected: nq,
                got: sigma.len().min(dam.len()).min(k0.len()),
            });
        }
        if let Some(es) = &mut self.effective_stress {
            // the undamaged stress of the parent's return (UD-139)
            es.get_mut(ty, ghost)?.copy_from(&sigma)?;
        }
        if grad_u.nb_component() != n
            || sigma.nb_component() != n
            || inel.as_ref().is_some_and(|a| a.nb_component() != n)
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialMazars::computeStress: components",
                expected: n,
                got: sigma.nb_component(),
            });
        }
        // a part: contiguous points of the copies (`first` = index of its
        // first point, to read the inelastic strain)
        let part = |first: usize,
                    (g_part, mut sigma, mut dam, mut ehat): (
            Tuples<'_, Real>,
            Tuples<'_, Real>,
            Tuples<'_, Real>,
            Tuples<'_, Real>,
        )|
         -> Result<()> {
            let g_part = g_part.into_slice();
            let len = dam.as_mut_slice().len();
            for k in 0..len {
                let q = first + k;
                let r = k * n..(k + 1) * n;
                let g: &mut Matrix<DIM, DIM> = as_matrix_mut(&mut g_part[r.clone()])?;
                let p: Option<&Matrix<DIM, DIM>> = inel
                    .as_ref()
                    .map(|a| as_matrix(&a.as_slice()[q * n..(q + 1) * n]))
                    .transpose()?;
                if let Some(p) = p {
                    *g = g.sub(p);
                }
                let inel_trace = p.map_or(0., SquareMatrixExt::trace);
                let e3 = mazars_strain_3d(g, inel_trace, delta_t.as_slice()[q] * alpha, mode);
                let (fdiag, e) = principal_strains_and_ehat3(&e3)?;
                ehat.as_mut_slice()[k] = e;
                // UD-138: the damage of an earlier trial of the step is not
                // kept
                if committed {
                    dam.as_mut_slice()[k] = prev_dam.as_slice()[q];
                }
                let d = &mut dam.as_mut_slice()[k];
                if damage_in_cs {
                    *d = mazars_damage_on_quad(&params, &el, e, k0.as_slice()[q], &fdiag, *d);
                }
                if !non_local {
                    if !damage_in_cs {
                        let (fdiag, _) = principal_strains_and_ehat3(&e3)?;
                        *d = mazars_damage_on_quad(&params, &el, e, k0.as_slice()[q], &fdiag, *d);
                    }
                    let f = 1. - *d;
                    for s in &mut sigma.as_mut_slice()[r] {
                        *s *= f;
                    }
                    if let Some(p) = p {
                        *g = g.add(p);
                    }
                }
            }
            Ok(())
        };
        let items = (
            Tuples::new(grad_u.as_mut_slice(), n),
            Tuples::new(sigma.as_mut_slice(), n),
            Tuples::new(dam.as_mut_slice(), 1),
            Tuples::new(ehat.as_mut_slice(), 1),
        );
        exec.try_for_each(items, 512, part)?;
        let base = self.parent.base_mut();
        copy_back(exec, base.grad_u.get_mut_with(exec, ty, ghost)?, &grad_u)?;
        copy_back(
            exec,
            base.sigma_field_mut()?.get_mut_with(exec, ty, ghost)?,
            &sigma,
        )?;
        copy_back(
            exec,
            self.damage.damage.get_mut_with(exec, ty, ghost)?,
            &dam,
        )?;
        copy_back(exec, self.ehat.get_mut_with(exec, ty, ghost)?, &ehat)?;
        Ok(())
    }
}

/// A copy of `a` made in parts of `exec`'s threads (the same values as
/// `Array::clone`; the id is not needed by the local copies).
fn clone_with(exec: &Exec, a: &Array<Real>) -> Result<Array<Real>> {
    // `vec!` of zeros takes zeroed pages from the allocator without a
    // write pass; the parts then touch (and copy into) their own pages
    let mut c = Array::from_vec(vec![0.0; a.as_slice().len()], a.nb_component())?;
    let items = (
        Tuples::new(c.as_mut_slice(), 1),
        akantu_core::exec::TuplesRef::new(a.as_slice(), 1),
    );
    exec.for_each(items, 65_536, |_, (mut d, s)| {
        d.as_mut_slice().copy_from_slice(s.as_slice());
    });
    Ok(c)
}

/// `dst.copy_from(src)` with the copy in parts of `exec`'s threads (the
/// same checks and errors first).
fn copy_back(
    exec: &Exec,
    mut dst: akantu_core::array_shared::ArrayMut<'_, Real>,
    src: &Array<Real>,
) -> Result<()> {
    if dst.len() != src.len() || dst.nb_component() != src.nb_component() {
        return dst.copy_from(src);
    }
    let items = (
        Tuples::new(dst.as_mut_slice(), 1),
        akantu_core::exec::TuplesRef::new(src.as_slice(), 1),
    );
    exec.for_each(items, 65_536, |_, (mut d, s)| {
        d.as_mut_slice().copy_from_slice(s.as_slice());
    });
    Ok(())
}

/// Name of the effective-stress internal of `plastic_mazars` (UD-139).
pub const EFFECTIVE_STRESS: &str = "effective_stress";

impl<const DIM: usize> MaterialMazarsElastic<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(
        Self,
        [parent],
        [
            crate::random_field_param!(Self, "K0", ParameterAccess::PARSABLE, "K0", k0),
            crate::real_param!(Self, "At", ParameterAccess::PARSABLE, "At", params.at),
            crate::real_param!(Self, "Ac", ParameterAccess::PARSABLE, "Ac", params.ac),
            crate::real_param!(Self, "Bc", ParameterAccess::PARSABLE, "Bc", params.bc),
            crate::real_param!(Self, "Bt", ParameterAccess::PARSABLE, "Bt", params.bt),
            crate::real_param!(Self, "beta", ParameterAccess::PARSABLE, "beta", params.beta),
        ]
    );
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(plane_stress Self, [parent], [
        crate::random_field_param!(Self, "K0", ParameterAccess::PARSABLE, "K0", k0),
        crate::real_param!(Self, "At", ParameterAccess::PARSABLE, "At", params.at),
        crate::real_param!(Self, "Ac", ParameterAccess::PARSABLE, "Ac", params.ac),
        crate::real_param!(Self, "Bc", ParameterAccess::PARSABLE, "Bc", params.bc),
        crate::real_param!(Self, "Bt", ParameterAccess::PARSABLE, "Bt", params.bt),
        crate::real_param!(Self, "beta", ParameterAccess::PARSABLE, "beta", params.beta),
    ]);
}

impl<const DIM: usize> MaterialVonMisesMazars<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(
        Self,
        [parent.elastic],
        [
            crate::real_param!(
                Self,
                "h",
                ParameterAccess::PARSMOD,
                "Hardening  modulus",
                parent.h
            ),
            crate::real_param!(
                Self,
                "sigma_y",
                ParameterAccess::PARSMOD,
                "Yield stress",
                parent.sigma_y
            ),
            crate::random_field_param!(Self, "K0", ParameterAccess::PARSABLE, "K0", k0),
            crate::real_param!(Self, "At", ParameterAccess::PARSABLE, "At", params.at),
            crate::real_param!(Self, "Ac", ParameterAccess::PARSABLE, "Ac", params.ac),
            crate::real_param!(Self, "Bc", ParameterAccess::PARSABLE, "Bc", params.bc),
            crate::real_param!(Self, "Bt", ParameterAccess::PARSABLE, "Bt", params.bt),
            crate::real_param!(Self, "beta", ParameterAccess::PARSABLE, "beta", params.beta),
        ]
    );
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(plane_stress Self, [parent.elastic], [
        crate::real_param!(Self, "h", ParameterAccess::PARSMOD, "Hardening  modulus", parent.h),
        crate::real_param!(Self, "sigma_y", ParameterAccess::PARSMOD, "Yield stress", parent.sigma_y),
        crate::random_field_param!(Self, "K0", ParameterAccess::PARSABLE, "K0", k0),
        crate::real_param!(Self, "At", ParameterAccess::PARSABLE, "At", params.at),
        crate::real_param!(Self, "Ac", ParameterAccess::PARSABLE, "Ac", params.ac),
        crate::real_param!(Self, "Bc", ParameterAccess::PARSABLE, "Bc", params.bc),
        crate::real_param!(Self, "Bt", ParameterAccess::PARSABLE, "Bt", params.bt),
        crate::real_param!(Self, "beta", ParameterAccess::PARSABLE, "beta", params.beta),
    ]);
}

impl<const DIM: usize> MaterialParams for MaterialMazarsElastic<DIM> {
    fn params_base(&self) -> &MaterialBase {
        self.parent.base()
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        self.parent.base_mut()
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        if DIM == 2 {
            Self::PARAMS_2D
        } else {
            Self::PARAMS
        }
    }
    fn update_params(&mut self) -> Result<()> {
        self.parent.parent_update_params()
    }
}

impl<const DIM: usize> MaterialParams for MaterialVonMisesMazars<DIM> {
    fn params_base(&self) -> &MaterialBase {
        self.parent.base()
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        self.parent.base_mut()
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        if DIM == 2 {
            Self::PARAMS_2D
        } else {
            Self::PARAMS
        }
    }
    fn update_params(&mut self) -> Result<()> {
        self.parent.parent_update_params()
    }
}

crate::impl_material_registry!(MaterialMazarsElastic<DIM>, const DIM);
crate::impl_material_registry!(MaterialVonMisesMazars<DIM>, const DIM);

impl<const DIM: usize, P> ConstitutiveLaw for MaterialMazars<DIM, P>
where
    P: DamageParent<DIM>,
    Self: Parsable + MaterialParams,
{
    fn name(&self) -> &str {
        self.parent.name()
    }
    fn law_id(&self) -> &str {
        self.parent.law_id()
    }
    fn law_type(&self) -> &str {
        self.parent.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.parent.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.parent.element_filter_mut()
    }
    /// The base's `(dimension, kind)` restriction for the law's own
    /// internals too (upstream `registerInternal` / `InternalField` with the
    /// law's dimension and `_ek_regular`; row CL-002).
    fn internal_restriction(&self) -> Option<akantu_model::internal_field::TypeRestriction> {
        Some(Material::base(self).internal_restriction())
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.parent.for_each_internal(f);
        self.damage.for_each(f);
        f("K0", InternalFieldRef::Real(&self.k0));
        f("Ehat", InternalFieldRef::Real(&self.ehat));
        if let Some(es) = &self.effective_stress {
            f(EFFECTIVE_STRESS, InternalFieldRef::Real(es));
        }
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.parent.for_each_internal_mut(f)?;
        self.damage.for_each_mut(f)?;
        f("K0", InternalFieldMut::Real(&mut self.k0))?;
        f("Ehat", InternalFieldMut::Real(&mut self.ehat))?;
        if let Some(es) = &mut self.effective_stress {
            f(EFFECTIVE_STRESS, InternalFieldMut::Real(es))?;
        }
        Ok(())
    }
}

impl<const DIM: usize, P> Material for MaterialMazars<DIM, P>
where
    P: DamageParent<DIM>,
    Self: Parsable + MaterialParams,
{
    fn base(&self) -> &MaterialBase {
        self.parent.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.parent.base_mut()
    }

    /// Upstream `MaterialElastic::initMaterial` (inherited by every parent).
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.parent.base_mut().init_history()?;
        let blocks = self.parent.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update_params()?;
        self.parent.base_mut().set_init();
        if DIM == 1 {
            self.parent.elastic_mut().thermal.nu = 0.;
        }
        self.update_params()
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        if self.parent.base().finite_deformation
            && self.parent.inelastic_strain().is_some()
            && self.options_ref().finite_strain_plasticity == FiniteStrainPlasticity::Unsupported
        {
            return Err(Error::Unsupported(
                "plastic_mazars with finite_deformation: no reviewed finite-strain J2 formulation \
                 (UD-030); FiniteStrainPlasticity::UpstreamLegacy reproduces upstream"
                    .into(),
            ));
        }
        if self.parent.base().finite_deformation
            && self.options_ref().finite_strain_damage == FiniteStrainDamage::Unsupported
        {
            return Err(Error::Unsupported(
                "mazars / plastic_mazars with finite_deformation: no reviewed objective \
                 finite-strain damage formulation (UD-142); \
                 FiniteStrainDamage::UpstreamLegacy reproduces upstream"
                    .into(),
            ));
        }
        if self.options_ref().damage_thermal_refresh {
            let exec = crate::material::points_exec(self, ctx);
            self.parent
                .elastic_mut()
                .compute_thermal_with(exec, ty, ghost)?;
        }
        let previous_sigma = match &self.effective_stress {
            Some(es) if self.options_ref().plastic_mazars_effective_stress => {
                Some(es.previous(ty, ghost)?.clone())
            }
            _ => None,
        };
        let exec = crate::material::points_exec(self, ctx);
        self.parent
            .stress_kernel_from(exec, ty, ghost, previous_sigma)?;
        self.damage_block(exec, ty, ghost)
    }

    /// A point's stress (the parent's kernel), equivalent strain and damage
    /// read only that point's values (copies of the blocks, written back
    /// when every point succeeded) and the law's constants; the thermal
    /// refresh runs per block before the loops. The non-local variant runs
    /// the same local loop (`Ehat` only) before its own averaging phase.
    fn independent_points(&self) -> bool {
        true
    }

    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let parent = &self.parent;
        self.damage.tangent(
            ty,
            ghost,
            tangent_stiffness_voigt_size(DIM),
            tangent,
            &|t| parent.tangent_on_quad(t),
        )
    }

    /// The densities on the model's threads (computed at every step by the
    /// energy update).
    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let exec = crate::material::points_exec(self, ctx);
        self.parent
            .parent_potential_energy(exec, &self.damage.damage, ty)
    }

    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        match energy_id {
            "dissipated" => self.damage.dissipated_energy(self.parent.base(), ctx),
            "potential" => self.potential_energy(ctx),
            other => Ok(self.parent.parent_energy(ctx, other)?.unwrap_or(0.0)),
        }
    }

    /// The law's density at the element's points (consistent with the
    /// whole-field energy, UD-032).
    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let points = super::damage::element_points(self.parent.base(), ctx, element)?;
        self.parent
            .parent_energy_densities(&self.damage.damage, element.ty(), points)
    }

    /// Upstream `MaterialDamage::updateEnergies`: the parent's update
    /// (plastic work), `computePotentialEnergy`, the dissipated energy.
    fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.parent
            .parent_update_energies(crate::material::points_exec(self, ctx), ty)?;
        self.compute_potential_energy(ctx, ty)?;
        let options = self.parent.elastic().options;
        let thermal = if super::damage::thermal_work_enabled(&options) {
            Some(self.parent.thermal_work(&self.damage.damage, ty)?)
        } else {
            None
        };
        let work = super::damage::DamageWork::new(&options, thermal.as_deref());
        let exec = crate::material::points_exec(self, ctx);
        self.damage
            .update_energies::<DIM>(exec, self.parent.base(), ty, work)
    }

    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.parent.elastic().push_wave_speed(element)
    }

    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.parent.elastic().shear_wave_speed(element)
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    /// Upstream `MaterialDamage::hasStiffnessMatrixChanged`: always `true`.
    fn has_stiffness_matrix_changed(&self) -> bool {
        true
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.parent.elastic().options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.parent.elastic_mut().options)
    }

    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        self.parent.elastic_mut().compute_all_cauchy_stresses(ghost)
    }
}

/// Register `mazars` and `plastic_mazars` (upstream
/// `instantiateMaterial<MaterialMazars_>("mazars")`,
/// `instantiateMaterial<MaterialVonMisesMazars>("plastic_mazars")`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "mazars",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialMazarsElastic::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )?;
    factory.register(
        "plastic_mazars",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialVonMisesMazars::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}
