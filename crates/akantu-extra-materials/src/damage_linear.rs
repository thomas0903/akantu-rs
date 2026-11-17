// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_damage/material_damage_linear.hh, material_damage_linear.cc,
// material_damage_linear_inline_impl.hh.

//! `damage_linear` (XMAT-004): isotropic damage with a linear softening in
//! terms of a positive-part equivalent strain (upstream
//! `MaterialDamageLinear<dim> : MaterialDamage<dim>`).
//!
//! Per quadrature point (`computeStressOnQuad`, upstream order):
//!
//! 1. principal values `λ_i` of the strain (see UD-576 below) as a 3 × 3
//!    tensor; `ε̂ = √(⟨λ_0⟩₊² + ⟨λ_1⟩₊² + ⟨λ_2⟩₊²)` (`⟨x⟩₊ = std::max(0, x)`);
//! 2. `σ = MaterialElastic::computeStressOnQuad(∇u)` (`σ_th = 0`: the law
//!    never runs the thermal pre-step, as upstream);
//! 3. if `ε̂ − K > 0`: `d = std::min(d(ε̂), 1)`, `K = ε̂`, with the
//!    linear-softening damage (default, UD-780)
//!    `d(ε̂) = (ε_max/ε̂)·(ε̂ − ε_min)/(ε_max − ε_min)` or upstream's
//!    `d(ε̂) = (ε̂ − ε_min)/(ε_max − ε_min)·(ε̂/ε_max)`;
//! 4. `σ *= 1 − d`.
//!
//! `ε_min = Sigc/E`, `ε_max = 2·Gc/Sigc + ε_min` are computed by
//! `initMaterial`, which also sets the default (and every value) of the
//! threshold internal `K` to `ε_min`. Parameters `Sigc` (1e5) and `Gc` (2),
//! parsable only.
//!
//! # Mechanics of the default law
//!
//! Under uniaxial stress with `σ = E·ε` (`ν = 0`, or 1-D), `ε̂ = ε` and the
//! default damage gives the bilinear curve the parameters define: `σ = Eε`
//! up to the peak `(ε_min, Sigc)`, then the straight softening line
//! `σ = Sigc·(ε_max − ε)/(ε_max − ε_min)` down to `(ε_max, 0)` (for
//! `ε ≥ ε_max`, `d = 1`). The area of the softening branch is
//! `½·Sigc·(ε_max − ε_min) = Gc`, and the energy dissipated per unit volume
//! up to complete failure (the whole area under the curve, all the stored
//! elastic energy being released) is `½·Sigc·ε_max = Gc + Sigc²/(2E)`.
//! Upstream's formula inverts the ratio `ε_max/ε̂`: the stress keeps rising
//! after `ε_min` (tests `damage_linear_*` in `tests/point_laws.rs`).
//!
//! # Upstream defects
//!
//! * **UD-780** (B): the inverted softening ratio above;
//!   [`ExtraLawOptions::damage_linear_linear_softening`] (default) is the
//!   linear softening law, the compat mode upstream's formula.
//! * **UD-575** (B): the quadrature loop never advances the `K` pointer, so
//!   every point of a block shares the first point's threshold;
//!   [`ExtraLawOptions::damage_linear_threshold_per_point`] (default)
//!   gives each point its own.
//! * **UD-576** (A for the overwrite, B for the strain): the principal
//!   values come from the removed `Math::matrix33_eigenvalues(grad_u)`,
//!   which reads the `dim × dim` gradient as a 3 × 3 matrix (out-of-bounds
//!   read for dim < 3), takes the non-symmetric `∇u` and — Akantu 4
//!   `matrixEig` → LAPACK `dgeev` on the array itself — overwrites the
//!   stored `∇u` with its Schur form before the stress is computed from it
//!   (in 2-D through a 4-value view: heap corruption). Both modes embed the
//!   block in a zero 3 × 3 matrix **on a copy** and use Eigen's 3 × 3
//!   `EigenSolver` (real parts); the stress always uses the unmodified
//!   `∇u` (the overwrite is not reproduced: category A).
//!   [`ExtraLawOptions::damage_linear_symmetric_strain`] (default) uses
//!   `sym(∇u)`, the compat mode the raw `∇u` (upstream's eigenvalues only).

use akantu_core::tensor::{Matrix, as_matrix, as_matrix_mut};
use akantu_core::{ElementType, GhostType, Real, Result};
use akantu_linalg::dense::eig3_fixed;
use akantu_model::InternalField;
use akantu_model::constitutive_law::{InternalFieldMut, InternalFieldRef, LawContext, LawSpec};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};
use akantu_solid::Material;
use akantu_solid::material::{MaterialBase, MaterialFactory, MaterialParams};
use akantu_solid::materials::damage::DamageFields;
use akantu_solid::materials::elastic::MaterialElastic;

use crate::kernels::{check_lengths, scale_by_integrity, std_max, std_min};
use crate::options::ExtraLawOptions;

/// `damage_linear` (upstream `MaterialDamageLinear<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialDamageLinear<const DIM: usize> {
    /// The elastic parent.
    pub elastic: MaterialElastic<DIM>,
    /// `damage` (history), `damage dissipated energy`, `integral of sigma`.
    pub damage: DamageFields,
    /// Threshold `K` per point (default `ε_min` after `initMaterial`).
    pub k: InternalField<Real>,
    /// `Gc`.
    pub gc: Real,
    /// `Sigc`.
    pub sigc: Real,
    /// `ε_min = Sigc/E` (set by `initMaterial`).
    pub eps_min: Real,
    /// `ε_max = 2·Gc/Sigc + ε_min` (set by `initMaterial`).
    pub eps_max: Real,
    /// Extra-materials defect switches (UD-575, UD-576, UD-780).
    pub extra_options: ExtraLawOptions,
}

impl<const DIM: usize> MaterialDamageLinear<DIM> {
    /// New law (upstream constructor chain).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "damage_linear".into();
        Ok(Self {
            elastic,
            damage: DamageFields::new()?,
            k: InternalField::new("K", 1, 0.0)?,
            gc: 2.,
            sigc: 1e5,
            eps_min: 0.,
            eps_max: 0.,
            extra_options: ExtraLawOptions::default(),
        })
    }

    /// Step 1 of the module docs: `ε̂`.
    ///
    /// # Errors
    /// Eigen-solver failures (non-finite input).
    pub fn equivalent_strain(&self, grad_u: &Matrix<DIM, DIM>) -> Result<Real> {
        let sym = self.extra_options.damage_linear_symmetric_strain;
        let mut a = [[0.; 3]; 3];
        for (j, col) in grad_u.iter().enumerate() {
            for (i, &g) in col.iter().enumerate() {
                a[j][i] = if sym { 0.5 * (g + grad_u[i][j]) } else { g };
            }
        }
        let f = eig3_fixed(&a)?;
        let p = [std_max(0., f[0]), std_max(0., f[1]), std_max(0., f[2])];
        Ok((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
    }

    /// The damage of the softening branch at `ε̂ > K ≥ ε_min` before the
    /// `std::min(·, 1)` clamp (step 3 of the module docs, UD-780).
    #[must_use]
    pub fn softening_damage(&self, ehat: Real) -> Real {
        if self.extra_options.damage_linear_linear_softening {
            self.eps_max / ehat * ((ehat - self.eps_min) / (self.eps_max - self.eps_min))
        } else {
            // UD-780: upstream's inverted ratio
            (ehat - self.eps_min) / (self.eps_max - self.eps_min) * (ehat / self.eps_max)
        }
    }

    /// Steps 2–4 of the module docs on one point: returns the new `(d, K)`
    /// and writes the damaged stress into `sigma`.
    ///
    /// # Errors
    /// Eigen-solver failures.
    pub fn stress_on_quad(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        sigma: &mut Matrix<DIM, DIM>,
        damage: Real,
        k: Real,
    ) -> Result<(Real, Real)> {
        let ehat = self.equivalent_strain(grad_u)?;
        self.elastic.constants().stress_on_quad(grad_u, 0., sigma);
        let (mut dam, mut k) = (damage, k);
        let fd = ehat - k;
        if fd > 0. {
            dam = self.softening_damage(ehat);
            dam = std_min(dam, 1.);
            k = ehat;
        }
        scale_by_integrity(sigma, dam);
        Ok((dam, k))
    }

    pub(crate) fn visit_own(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        f("K", InternalFieldRef::Real(&self.k));
    }

    pub(crate) fn visit_own_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        f("K", InternalFieldMut::Real(&mut self.k))
    }

    /// Upstream `initMaterial` after `MaterialDamage::initMaterial`.
    pub(crate) fn after_init(&mut self) -> Result<()> {
        self.eps_min = self.sigc / self.elastic.thermal.e;
        self.eps_max = 2. * self.gc / self.sigc + self.eps_min;
        self.k.set_default_value(self.eps_min);
        Ok(())
    }

    /// Upstream `computeStress(type, ghost)`.
    pub(crate) fn stress_impl(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let n = DIM * DIM;
        let per_point = self.extra_options.damage_linear_threshold_per_point;
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let mut sigma = self.elastic.base().stress.get(ty, ghost)?.clone();
        let mut dam = self.damage.damage.get(ty, ghost)?.clone();
        let mut k = self.k.get(ty, ghost)?.clone();
        let nq = grad_u.len();
        check_lengths(
            "MaterialDamageLinear::computeStress",
            nq,
            &[sigma.len(), dam.len(), k.len()],
        )?;
        if per_point {
            // every point from its own values (copies of the blocks,
            // written back when every point succeeded): parts on the
            // model's threads (D-123)
            check_lengths(
                "MaterialDamageLinear::computeStress: components",
                n,
                &[grad_u.nb_component(), sigma.nb_component()],
            )?;
            let items = (
                akantu_core::exec::TuplesRef::new(grad_u.as_slice(), n),
                akantu_core::exec::Tuples::new(sigma.as_mut_slice(), n),
                akantu_core::exec::Tuples::new(dam.as_mut_slice(), 1),
                akantu_core::exec::Tuples::new(k.as_mut_slice(), 1),
            );
            let this = &*self;
            akantu_solid::material::points_exec(this, ctx).try_for_each(
                items,
                512,
                |_, (g, mut s, mut dam, mut k)| {
                    let points = g
                        .as_slice()
                        .chunks_exact(n)
                        .zip(s.as_mut_slice().chunks_exact_mut(n))
                        .zip(dam.as_mut_slice().iter_mut().zip(k.as_mut_slice()));
                    for ((g, s), (dam, k)) in points {
                        let (d, knew) =
                            this.stress_on_quad(as_matrix(g)?, as_matrix_mut(s)?, *dam, *k)?;
                        *dam = d;
                        *k = knew;
                    }
                    Ok(())
                },
            )?;
        } else {
            for (q, (g, s)) in grad_u
                .as_slice()
                .chunks_exact(n)
                .zip(sigma.as_mut_slice().chunks_exact_mut(n))
                .enumerate()
            {
                // UD-575: upstream reads and writes the first point's threshold
                let kq = if per_point { q } else { 0 };
                let (d, knew) = self.stress_on_quad(
                    as_matrix(g)?,
                    as_matrix_mut(s)?,
                    dam.as_slice()[q],
                    k.as_slice()[kq],
                )?;
                dam.as_mut_slice()[q] = d;
                k.as_mut_slice()[kq] = knew;
            }
        }
        self.elastic
            .base_mut()
            .stress
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        self.damage.damage.get_mut(ty, ghost)?.copy_from(&dam)?;
        self.k.get_mut(ty, ghost)?.copy_from(&k)?;
        Ok(())
    }

    /// With per-point thresholds (the corrected default) a point reads
    /// only its own gradient, damage and threshold (copies of the blocks)
    /// and the law's constants. Upstream mode (UD-575): every point reads
    /// and writes the first point's threshold, the loop stays serial.
    pub(crate) fn points_are_independent(&self) -> bool {
        self.extra_options.damage_linear_threshold_per_point
    }

    const PARAMS: &'static [ParamDescriptor<Self>] = akantu_solid::elastic_parent_params!(
        Self,
        [elastic],
        [
            akantu_solid::real_param!(
                Self,
                "Sigc",
                ParameterAccess::PARSABLE,
                "Sigma Critique",
                sigc
            ),
            akantu_solid::real_param!(Self, "Gc", ParameterAccess::PARSABLE, "Gc", gc),
        ]
    );
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = akantu_solid::elastic_parent_params!(plane_stress Self, [elastic], [
        akantu_solid::real_param!(Self, "Sigc", ParameterAccess::PARSABLE, "Sigma Critique", sigc),
        akantu_solid::real_param!(Self, "Gc", ParameterAccess::PARSABLE, "Gc", gc),
    ]);
}

impl<const DIM: usize> MaterialParams for MaterialDamageLinear<DIM> {
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

crate::damage_law::impl_damage_law!(MaterialDamageLinear);

/// Register `damage_linear` (upstream `INSTANTIATE_MATERIAL(damage_linear,
/// MaterialDamageLinear)`).
///
/// # Errors
/// A duplicate registration.
pub fn register(factory: &mut MaterialFactory, options: ExtraLawOptions) -> Result<()> {
    crate::damage_law::register_law!(factory, "damage_linear", MaterialDamageLinear, options)
}
