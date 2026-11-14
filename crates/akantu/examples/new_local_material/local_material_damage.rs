// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// new_material/local_material_damage.hh, local_material_damage.cc,
// local_material_damage_inline_impl.hh.

//! `local_damage`: a user material written against the [`Material`]
//! trait (upstream: a subclass of `Material` registered in the
//! `MaterialFactory`).
//!
//! Isotropic linear elasticity degraded by a scalar damage `d` per
//! quadrature point:
//!
//! * `σ₀ = λ tr(∇u) I + μ (∇u + ∇uᵀ)`, energy release `Y = ½ σ₀ : ∇u`;
//! * if `Y − Y_d − S_d d > 0` then `d = (Y − Y_d)/S_d`; `d = min(d, 1)`;
//! * `σ = (1 − d) σ₀`;
//! * potential energy density `½ σ : ∇u` (see below);
//! * celerity `√((2μ + λ)/ρ)` (stable time step).
//!
//! Parameters: `E` (parsable, default 0), `nu` (parsable, default 0.5),
//! `Yd` (default 50), `Sd` (default 5000), read-only `lambda`, `mu`,
//! `kapa` (computed by `initMaterial`). Internal field `damage` (1
//! component, history-free).
//!
//! **UD-472** (D-003 category B): upstream's energy density is
//! `½ σ : (∇u − I)` (`grad_u(i, j) - (i == j)`), which subtracts
//! `½ tr σ` from the stored energy (a leftover of a deformation-gradient
//! convention). The corrected `½ σ : ∇u` is the default;
//! [`LocalMaterialDamage::upstream_energy`] reproduces upstream (the
//! example sets it with `--upstream-compat`).

use std::sync::Arc;

use akantu::core::{
    Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result, SliceRef,
};
use akantu::model::InternalField;
use akantu::model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu::parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};
use akantu::solid::material::MaterialParams;
use akantu::solid::{Material, MaterialBase, MaterialFactory};

/// Registered type name (`material local_damage [ … ]`).
pub const TYPE: &str = "local_damage";

/// Upstream `LocalMaterialDamage`.
#[derive(Clone, Debug)]
pub struct LocalMaterialDamage {
    base: MaterialBase,
    /// Young's modulus.
    e: Real,
    /// Poisson's ratio.
    nu: Real,
    /// First Lamé coefficient.
    lambda: Real,
    /// Second Lamé coefficient (shear modulus).
    mu: Real,
    /// Resistance to damage.
    yd: Real,
    /// Damage threshold.
    sd: Real,
    /// Bulk modulus.
    kpa: Real,
    /// Damage internal variable.
    pub damage: InternalField<Real>,
    /// UD-472: reproduce upstream's `½ σ : (∇u − I)` energy density.
    pub upstream_energy: bool,
}

/// A `Real` parameter stored in field `get`/`set` (upstream
/// `registerParam(name, member, default, access, description)`).
macro_rules! param {
    ($name:literal, $access:expr, $desc:literal, $f:ident, writable) => {
        ParamDescriptor::new(
            $name,
            ParamKind::Real,
            $access,
            $desc,
            |s: &LocalMaterialDamage| ParamValue::Real(s.$f),
            Some(|s: &mut LocalMaterialDamage, v: ParamValue| {
                s.$f = v.as_real()?;
                Ok(())
            }),
        )
    };
    ($name:literal, $desc:literal, $f:ident) => {
        ParamDescriptor::new(
            $name,
            ParamKind::Real,
            ParameterAccess::READABLE,
            $desc,
            |s: &LocalMaterialDamage| ParamValue::Real(s.$f),
            None,
        )
    };
}

impl LocalMaterialDamage {
    /// Parameters in upstream registration order.
    const PARAMS: &'static [ParamDescriptor<Self>] = &[
        param!(
            "E",
            ParameterAccess::PARSABLE,
            "Young's modulus",
            e,
            writable
        ),
        param!(
            "nu",
            ParameterAccess::PARSABLE,
            "Poisson's ratio",
            nu,
            writable
        ),
        param!("lambda", "First Lamé coefficient", lambda),
        param!("mu", "Second Lamé coefficient", mu),
        param!("kapa", "Bulk coefficient", kpa),
        param!("Yd", ParameterAccess::PARSMOD, "", yd, writable),
        param!("Sd", ParameterAccess::PARSMOD, "", sd, writable),
    ];

    /// Upstream constructor (`registerInternal("damage", 1)` and the
    /// parameters with their defaults).
    ///
    /// # Errors
    /// An invalid spatial dimension.
    pub fn new(spec: &LawSpec, mesh: &akantu::mesh::Mesh) -> Result<Self> {
        Ok(Self {
            base: MaterialBase::with_mesh(spec, TYPE, mesh)?,
            e: 0.,
            nu: 0.5,
            lambda: 0.,
            mu: 0.,
            yd: 50.,
            sd: 5000.,
            kpa: 0.,
            damage: InternalField::new("damage", 1, 0.0)?,
            upstream_energy: false,
        })
    }

    /// Damage of every quadrature point of `(ty, ghost)`.
    ///
    /// # Errors
    /// No such block.
    pub fn damage_of(&self, ty: ElementType, ghost: GhostType) -> Result<SliceRef<'_, Real>> {
        Ok(self.damage.get(ty, ghost)?.into_slice())
    }

    /// The Lamé constants and the bulk modulus from `E` and `nu` (the part
    /// of upstream `initMaterial` after `Material::initMaterial()`).
    pub fn update_lame(&mut self) {
        self.lambda = self.nu * self.e / ((1. + self.nu) * (1. - 2. * self.nu));
        self.mu = self.e / (2. * (1. + self.nu));
        self.kpa = self.lambda + 2. / 3. * self.mu;
    }

    /// `(λ, μ)`.
    #[must_use]
    pub fn lame(&self) -> (Real, Real) {
        (self.lambda, self.mu)
    }

    /// Values of the parameters registered parse-only (`E`, `nu`, upstream
    /// `_pat_parsable`: not readable through the registry), for the
    /// printout of the law (`std::cout << model.getMaterial(0)`).
    #[must_use]
    pub fn private_param(&self, name: &str) -> Option<ParamValue> {
        match name {
            "E" => Some(ParamValue::Real(self.e)),
            "nu" => Some(ParamValue::Real(self.nu)),
            _ => None,
        }
    }

    /// Upstream `computeStressOnQuad(grad_u, sigma, dam)`; `g` and `s` are
    /// column-major `dim × dim`.
    pub fn stress_on_quad(&self, dim: usize, g: &[Real], s: &mut [Real], dam: &mut Real) {
        Kernel::of(self).stress_on_quad(dim, g, s, dam);
    }

    /// Upstream `computePotentialEnergyOnQuad(grad_u, sigma, epot)`
    /// (UD-472 switch, see the module documentation).
    #[must_use]
    pub fn energy_on_quad(&self, dim: usize, g: &[Real], s: &[Real]) -> Real {
        let mut epot = 0.;
        for i in 0..dim {
            for j in 0..dim {
                let gij = if self.upstream_energy {
                    g[i + j * dim] - Real::from(u8::from(i == j))
                } else {
                    g[i + j * dim]
                };
                epot += s[i + j * dim] * gij;
            }
        }
        epot * 0.5
    }
}

impl MaterialParams for LocalMaterialDamage {
    fn params_base(&self) -> &MaterialBase {
        &self.base
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        Self::PARAMS
    }
    /// Not overridden upstream: `Material::updateInternalParameters`.
    fn update_params(&mut self) -> Result<()> {
        self.base.update_internal_parameters()
    }
}

akantu::solid::impl_material_registry!(LocalMaterialDamage);

impl ConstitutiveLaw for LocalMaterialDamage {
    fn name(&self) -> &str {
        &self.base.name
    }
    fn law_id(&self) -> &str {
        &self.base.id
    }
    fn law_type(&self) -> &str {
        &self.base.law_type
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        &self.base.element_filter
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        &mut self.base.element_filter
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        for (n, field) in self.base.internals() {
            f(n, InternalFieldRef::Real(field));
        }
        f("damage", InternalFieldRef::Real(&self.damage));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        for field in self.base.internals_mut() {
            let name = field.name().to_owned();
            f(&name, InternalFieldMut::Real(field))?;
        }
        f("damage", InternalFieldMut::Real(&mut self.damage))
    }
}

/// The constants of the stress kernel, copied out of the law so that
/// `compute_stress` can borrow the internal fields in place
/// (no per-call copies of the arrays).
#[derive(Clone, Copy, Debug)]
struct Kernel {
    lambda: Real,
    mu: Real,
    yd: Real,
    sd: Real,
}

impl Kernel {
    fn of(m: &LocalMaterialDamage) -> Self {
        Self {
            lambda: m.lambda,
            mu: m.mu,
            yd: m.yd,
            sd: m.sd,
        }
    }

    fn stress_on_quad(&self, dim: usize, g: &[Real], s: &mut [Real], dam: &mut Real) {
        // Eigen trace of a dynamic matrix: sequential sum of the diagonal
        let mut trace = g[0];
        for i in 1..dim {
            trace += g[i + i * dim];
        }

        // σ_ij = λ (∇u)_kk δ_ij + μ (∇u_ij + ∇u_ji)
        for i in 0..dim {
            for j in 0..dim {
                let delta = Real::from(u8::from(i == j));
                s[i + j * dim] =
                    delta * self.lambda * trace + self.mu * (g[i + j * dim] + g[j + i * dim]);
            }
        }

        let mut y = 0.;
        for i in 0..dim {
            for j in 0..dim {
                y += s[i + j * dim] * g[i + j * dim];
            }
        }
        y *= 0.5;

        let fd = y - self.yd - self.sd * *dam;

        if fd > 0. {
            *dam = (y - self.yd) / self.sd;
        }
        // std::min(dam, 1.)
        if 1. < *dam {
            *dam = 1.;
        }

        let factor = 1. - *dam;
        for v in s.iter_mut() {
            *v *= factor;
        }
    }
}

impl Material for LocalMaterialDamage {
    fn base(&self) -> &MaterialBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }
    /// Downcast for the example's printout of the law (its parse-only
    /// parameters, [`LocalMaterialDamage::private_param`]).
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// Upstream `initMaterial`: `Material::initMaterial()` (history of
    /// `stress`/`grad_u`, internals resized), then the Lamé constants.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.base.init_history()?;
        let blocks = self.base.blocks(ctx);
        self.resize_internals(&blocks)?;
        self.base.update_internal_parameters()?;
        self.base.set_init();
        self.update_lame();
        Ok(())
    }

    /// Upstream `computeStress(el_type, ghost_type)`: the stress loop over
    /// the quadrature points with the damage of each point.
    fn compute_stress(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let dim = self.base.spatial_dimension;
        let n = dim * dim;
        let kernel = Kernel::of(self);
        let grad_u = self.base.grad_u.get(ty, ghost)?;
        let mut sigma = self.base.stress.get_mut(ty, ghost)?;
        let mut damage = self.damage.get_mut(ty, ghost)?;
        if sigma.len() != grad_u.len() || damage.len() != grad_u.len() {
            return Err(Error::DimensionMismatch {
                context: "LocalMaterialDamage::computeStress",
                expected: grad_u.len(),
                got: sigma.len().min(damage.len()),
            });
        }
        for ((g, s), d) in grad_u
            .as_slice()
            .chunks_exact(n)
            .zip(sigma.as_mut_slice().chunks_exact_mut(n))
            .zip(damage.as_mut_slice())
        {
            kernel.stress_on_quad(dim, g, s, d);
        }
        Ok(())
    }

    /// Not overridden upstream (`Material::computeTangentModuli`:
    /// `AKANTU_TO_IMPLEMENT`): the material is explicit-only.
    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        _ty: ElementType,
        _ghost: GhostType,
        _tangent: &mut akantu::core::Array<Real>,
    ) -> Result<()> {
        Err(Error::Unsupported(
            "local_damage: computeTangentModuli is not implemented (as upstream)".into(),
        ))
    }

    /// Upstream `computePotentialEnergy(el_type)` (not-ghost elements).
    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let g = GhostType::NotGhost;
        let dim = self.base.spatial_dimension;
        let n = dim * dim;
        let grad_u = self.base.grad_u.get(ty, g)?;
        let sigma = self.base.stress.get(ty, g)?;
        let density: Vec<Real> = grad_u
            .as_slice()
            .chunks_exact(n)
            .zip(sigma.as_slice().chunks_exact(n))
            .map(|(gu, s)| self.energy_on_quad(dim, gu, s))
            .collect();
        let mut epot = self.base.potential_energy.get_mut(ty, g)?;
        if epot.len() != density.len() {
            return Err(Error::DimensionMismatch {
                context: "LocalMaterialDamage::computePotentialEnergy",
                expected: density.len(),
                got: epot.len(),
            });
        }
        epot.as_mut_slice().copy_from_slice(&density);
        Ok(())
    }

    /// Upstream `getCelerity`: the push-wave speed `√((2μ + λ)/ρ)`.
    fn celerity(&self, _element: Element) -> Result<Real> {
        Ok(((2. * self.mu + self.lambda) / self.base.rho).sqrt())
    }

    /// Not overridden upstream (`AKANTU_TO_IMPLEMENT`).
    fn push_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::Unsupported(
            "local_damage: getPushWaveSpeed is not implemented (as upstream)".into(),
        ))
    }

    /// Not overridden upstream (`AKANTU_TO_IMPLEMENT`).
    fn shear_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::Unsupported(
            "local_damage: getShearWaveSpeed is not implemented (as upstream)".into(),
        ))
    }
}

/// Register `local_damage` in `factory` (upstream
/// `MaterialFactory::getInstance().registerAllocator("local_damage", …)`).
/// With `upstream_energy`, every created law reproduces UD-472.
///
/// # Errors
/// A duplicate registration.
pub fn register(factory: &mut MaterialFactory, upstream_energy: bool) -> Result<()> {
    factory.register(
        TYPE,
        Arc::new(
            move |spec: &LawSpec, services: &dyn ModelServices| -> Result<Box<dyn Material>> {
                let mut m = LocalMaterialDamage::new(spec, services.mesh())?;
                m.upstream_energy = upstream_energy;
                Ok(Box::new(m))
            },
        ),
    )
}
