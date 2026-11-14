// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_damage/material_brittle.hh, material_brittle.cc,
// material_brittle_inline_impl.hh.

//! `brittle` (XMAT-005): strain-rate dependent brittle damage (upstream
//! `MaterialBrittle<dim> : MaterialDamage<dim>`, "Josue", 2014).
//!
//! `computeStress(type, ghost)`: the velocity gradient `∇v` at the points
//! (internal `strain_rate_brittle`, `dim²`), then per point
//! (`computeStressOnQuad`, upstream order):
//!
//! 1. `σ = MaterialElastic::computeStressOnQuad(∇u)` (`σ_th = 0`);
//! 2. `ε̇_eq = √(Σ_ij 2/3·(½(∇v_ij + ∇v_ji) − δ_ij tr∇v/3)²)`, plus
//!    `2/3·(tr∇v/3)²` in 2-D (the out-of-plane deviatoric rate of plane
//!    strain), accumulated in that order; every square is upstream's
//!    `pow(x, 2.)`, evaluated with the bit-exact port of glibc's `pow`
//!    (not correctly rounded at `y = 2`: 1-ulp differences from `x·x` for
//!    ≈ 0.085 % of random inputs, measured);
//! 3. fracture stress `S_0` if `ε̇_eq ≤ E_0`, else the high-rate value
//!    (UD-577 below);
//! 4. `σ_eq` = the largest eigenvalue of `σ` (`EigenSolver` order, then
//!    `std::max` from the first);
//! 5. unless the law is non-local: `d = 1` if `σ_eq > fracture stress`,
//!    `d = std::min(d, 1)`, `σ *= 1 − d`.
//!
//! Parameters: `S_0` (157e6, parsable-modifiable), `E_0` (27e3), `A`
//! (1.622e-5), `B` (−1.3274), `C` (3.6544e4), `D` (−181.38e6) (parsable).
//! The header documents other defaults (`1.622e-11, −1.3274e-6,
//! 3.6544e-2, −181.38`: MPa and `ε̇` per millisecond); the code defaults
//! are kept (UD-577).
//!
//! # Upstream defects
//!
//! * **UD-577** (B): above `E_0` the code uses the constant `A`
//!   (`fracture_stress = A`, with the cubic commented out) although the
//!   header documents `A, B, C, D` as the fitting parameters of the
//!   high-rate critical stress and the original 2014 code evaluated
//!   `A ε̇³ + B ε̇² + C ε̇ + D` — continuous with `S_0` at `E_0` for the code
//!   defaults (`1.568e8` vs `1.57e8`), whereas `A = 1.622e-5` Pa damages any
//!   loaded point. [`ExtraLawOptions::brittle_polynomial_fracture_stress`]
//!   (default) evaluates the cubic with glibc's `pow` (bit-exact port,
//!   upstream's `pow(ε̇, 3) / pow(ε̇, 2)`); the compat mode uses `A`.
//! * The 2-D plane-stress out-of-plane rate (`ε̇_33` from `λ`, `μ`) of the
//!   original code is commented out upstream; the plane-strain term is used
//!   in every 2-D case (C, preserved).

use akantu_core::glibc_libm::pow;
use akantu_core::tensor::{Matrix, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{Array, ElementType, GhostType, Real, Result};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{InternalFieldMut, InternalFieldRef, LawContext, LawSpec};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};
use akantu_solid::Material;
use akantu_solid::material::{MaterialBase, MaterialFactory, MaterialParams};
use akantu_solid::materials::damage::DamageFields;
use akantu_solid::materials::elastic::MaterialElastic;

use crate::kernels::{check_lengths, eigenvalues, scale_by_integrity, std_max, std_min};
use crate::options::ExtraLawOptions;

/// `brittle` (upstream `MaterialBrittle<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialBrittle<const DIM: usize> {
    /// The elastic parent.
    pub elastic: MaterialElastic<DIM>,
    /// `damage` (history), `damage dissipated energy`, `integral of sigma`.
    pub damage: DamageFields,
    /// Velocity gradient per point (`strain_rate_brittle`, `dim²`).
    pub strain_rate: InternalField<Real>,
    /// `A` (cubic coefficient).
    pub a: Real,
    /// `B` (quadratic coefficient).
    pub b: Real,
    /// `C` (linear coefficient).
    pub c: Real,
    /// `D` (constant).
    pub d: Real,
    /// `E_0`: strain-rate threshold.
    pub e_0: Real,
    /// `S_0`: critical stress at low strain rates.
    pub s_0: Real,
    /// Extra-materials defect switches (UD-577).
    pub extra_options: ExtraLawOptions,
}

/// Result of the brittle point kernel before the damage update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrittleState {
    /// Largest principal stress `σ_eq`.
    pub sigma_equivalent: Real,
    /// Fracture stress at the point's strain rate.
    pub fracture_stress: Real,
}

impl<const DIM: usize> MaterialBrittle<DIM> {
    /// New law (upstream constructor chain).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        Self::with_type(spec, mesh, "brittle")
    }

    /// New law registered under `law_type` (non-local variant).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn with_type(
        spec: &LawSpec,
        mesh: Option<&akantu_mesh::Mesh>,
        law_type: &str,
    ) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = law_type.into();
        Ok(Self {
            elastic,
            damage: DamageFields::new()?,
            strain_rate: InternalField::new("strain_rate_brittle", DIM * DIM, 0.0)?,
            a: 1.622e-5,
            b: -1.3274,
            c: 3.6544e4,
            d: -181.38e6,
            e_0: 27e3,
            s_0: 157e6,
            extra_options: ExtraLawOptions::default(),
        })
    }

    /// Step 2 of the module docs: the equivalent strain rate of `∇v`.
    #[must_use]
    #[allow(clippy::needless_range_loop)] // upstream's (i, j) loops, kept for the sum order
    pub fn equivalent_strain_rate(grad_v: &Matrix<DIM, DIM>) -> Real {
        let mut rate = 0.;
        let volume_change_rate = grad_v.trace();
        if DIM == 2 {
            // upstream `pow(x, 2.)`: glibc's `pow` is not correctly rounded
            // at y = 2 and differs from `x * x` by one ulp for about 1 in
            // 1200 inputs, so its bit-exact port is used
            rate += 2. / 3. * pow(volume_change_rate / 3., 2.);
        }
        for i in 0..DIM {
            for j in 0..DIM {
                let delta = if i == j { volume_change_rate / 3. } else { 0. };
                // `(i == j) * volume_change_rate / 3.`: 0·x/3 is +0 or −0;
                // subtracting either zero leaves the value unchanged
                let x = 0.5 * (grad_v[j][i] + grad_v[i][j]) - delta;
                rate += 2. / 3. * pow(x, 2.);
            }
        }
        rate.sqrt()
    }

    /// Step 3 of the module docs.
    #[must_use]
    pub fn fracture_stress(&self, equivalent_strain_rate: Real) -> Real {
        if equivalent_strain_rate > self.e_0 {
            if self.extra_options.brittle_polynomial_fracture_stress {
                let r = equivalent_strain_rate;
                self.a * pow(r, 3.) + self.b * pow(r, 2.) + self.c * r + self.d
            } else {
                self.a
            }
        } else {
            self.s_0
        }
    }

    /// Steps 1–4 of the module docs (the stress written into `sigma`).
    ///
    /// # Errors
    /// Eigen-solver failures (non-finite input).
    pub fn stress_on_quad(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        grad_v: &Matrix<DIM, DIM>,
        sigma: &mut Matrix<DIM, DIM>,
    ) -> Result<BrittleState> {
        self.elastic.constants().stress_on_quad(grad_u, 0., sigma);
        let fracture_stress = self.fracture_stress(Self::equivalent_strain_rate(grad_v));
        let principal = eigenvalues(sigma)?;
        let mut sigma_equivalent = principal[0];
        for &p in &principal[1..] {
            sigma_equivalent = std_max(sigma_equivalent, p);
        }
        Ok(BrittleState {
            sigma_equivalent,
            fracture_stress,
        })
    }

    /// Step 5 of the module docs (upstream `computeDamageAndStressOnQuad`):
    /// returns the new damage.
    pub fn damage_and_stress_on_quad(
        sigma: &mut Matrix<DIM, DIM>,
        damage: Real,
        sigma_c: Real,
        fracture_stress: Real,
    ) -> Real {
        let mut dam = damage;
        if sigma_c > fracture_stress {
            dam = 1.;
        }
        dam = std_min(dam, 1.);
        scale_by_integrity(sigma, dam);
        dam
    }

    /// Velocity gradient of the block into `strain_rate_brittle`.
    ///
    /// # Errors
    /// No velocity field (static analyses) or gradient errors.
    pub fn update_strain_rate(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let filter = self
            .elastic
            .base()
            .element_filter
            .get(ty, ghost)?
            .as_slice()
            .to_vec();
        let mut gv = Array::new(DIM * DIM);
        ctx.services
            .gradient_on_integration_points("velocity", &mut gv, DIM, ty, ghost, &filter)?;
        let mut out = self.strain_rate.get_mut(ty, ghost)?;
        check_lengths(
            "MaterialBrittle::computeStress: strain rate",
            out.len(),
            &[gv.len()],
        )?;
        if !gv.is_empty() {
            out.copy_from(&gv)?;
        }
        Ok(())
    }

    /// The point loop of upstream `computeStress`; `sink` receives the
    /// point state (the non-local variant stores it).
    ///
    /// # Errors
    /// Missing blocks, size mismatches or eigen-solver failures.
    pub fn stress_loop(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        mut sink: impl FnMut(usize, BrittleState),
    ) -> Result<()> {
        let n = DIM * DIM;
        let non_local = self.elastic.base().is_non_local;
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let grad_v = self.strain_rate.get(ty, ghost)?.clone();
        let mut sigma = self.elastic.base().stress.get(ty, ghost)?.clone();
        let mut dam = self.damage.damage.get(ty, ghost)?.clone();
        check_lengths(
            "MaterialBrittle::computeStress",
            grad_u.len(),
            &[grad_v.len(), sigma.len(), dam.len()],
        )?;
        for (q, ((g, v), s)) in grad_u
            .as_slice()
            .chunks_exact(n)
            .zip(grad_v.as_slice().chunks_exact(n))
            .zip(sigma.as_mut_slice().chunks_exact_mut(n))
            .enumerate()
        {
            let s: &mut Matrix<DIM, DIM> = as_matrix_mut(s)?;
            let st = self.stress_on_quad(as_matrix(g)?, as_matrix(v)?, s)?;
            if !non_local {
                let d = &mut dam.as_mut_slice()[q];
                *d =
                    Self::damage_and_stress_on_quad(s, *d, st.sigma_equivalent, st.fracture_stress);
            }
            sink(q, st);
        }
        self.elastic
            .base_mut()
            .stress
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        self.damage.damage.get_mut(ty, ghost)?.copy_from(&dam)?;
        Ok(())
    }

    pub(crate) fn visit_own(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        f(
            "strain_rate_brittle",
            InternalFieldRef::Real(&self.strain_rate),
        );
    }

    pub(crate) fn visit_own_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        f(
            "strain_rate_brittle",
            InternalFieldMut::Real(&mut self.strain_rate),
        )
    }

    /// Upstream `initMaterial` after `MaterialDamage::initMaterial` (the
    /// internal is sized there; `updateInternalParameters` again).
    pub(crate) fn after_init(&mut self) -> Result<()> {
        MaterialParams::update_params(self)
    }

    /// Upstream `computeStress(type, ghost)`.
    pub(crate) fn stress_impl(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.update_strain_rate(ctx, ty, ghost)?;
        self.stress_loop(ty, ghost, |_, _| {})
    }
}

macro_rules! brittle_params {
    ($($ps:ident)?) => {
        akantu_solid::elastic_parent_params!($($ps)? Self, [elastic], [
            akantu_solid::real_param!(Self, "S_0", ParameterAccess::PARSMOD, "", s_0),
            akantu_solid::real_param!(Self, "E_0", ParameterAccess::PARSABLE, "Strain rate threshold", e_0),
            akantu_solid::real_param!(Self, "A", ParameterAccess::PARSABLE, "Polynome cubic constant", a),
            akantu_solid::real_param!(Self, "B", ParameterAccess::PARSABLE, "Polynome quadratic constant", b),
            akantu_solid::real_param!(Self, "C", ParameterAccess::PARSABLE, "Polynome linear constant", c),
            akantu_solid::real_param!(Self, "D", ParameterAccess::PARSABLE, "Polynome constant", d),
        ])
    };
}

impl<const DIM: usize> MaterialBrittle<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = brittle_params!();
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = brittle_params!(plane_stress);
}

impl<const DIM: usize> MaterialParams for MaterialBrittle<DIM> {
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
        MaterialParams::update_params(&mut self.elastic)
    }
}

crate::damage_law::impl_damage_law!(MaterialBrittle);

/// Register `brittle` (upstream `INSTANTIATE_MATERIAL(brittle,
/// MaterialBrittle)`).
///
/// # Errors
/// A duplicate registration.
pub fn register(factory: &mut MaterialFactory, options: ExtraLawOptions) -> Result<()> {
    crate::damage_law::register_law!(factory, "brittle", MaterialBrittle, options)
}
