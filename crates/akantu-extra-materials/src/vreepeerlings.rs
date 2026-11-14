// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_damage/material_vreepeerlings.hh, material_vreepeerlings_tmpl.hh,
// material_vreepeerlings_inline_impl.hh.

//! `vreepeerlings` (XMAT-007): rate-dependent isotropic damage with the
//! modified von Mises equivalent strain of de Vree / Peerlings (upstream
//! `MaterialVreePeerlings<dim, MaterialElastic> : MaterialDamage<dim>`).
//! Header-only upstream and never registered (no `INSTANTIATE_MATERIAL`);
//! the port registers it under its legacy-list keyword `vreepeerlings`
//! (UD-572).
//!
//! `computeStress(type, ghost)`: `MaterialElastic::computeStress` (thermal
//! pre-step and elastic stress), the velocity gradient `∇v` into
//! `strain-rate-vreepeerlings`, then per point (upstream order):
//!
//! * invariants of `∇u` and their rates: `I1 = tr ∇u`, `J2 = ½ε:ε −
//!   I1²/6` (`ε = sym ∇u`, accumulated as upstream), `İ1 = tr ∇v`, `J̇2`
//!   (UD-580);
//! * `t1 = (k − 1)/(1 − 2ν)`, `t2 = 12k/(1 + ν)²` (`k` = `Kct`):
//!   `ε̂ = t1·I1/(2k) + 1/(2k)·√(t1²I1² + t2·J2)`;
//! * when loading (`I1·İ1 > 0` or `J̇2 > 0`): `ε̂̇ = t1|İ1|/(2k) +
//!   1/(4k)·(2t1²I1İ1 + t2J̇2)/√(t1²I1² + t2J2)`, else UD-579;
//! * unless non-local: `κ₀ = C_rate(1 + (|ε̂̇|·A_rate)^{B_rate})(1 − r) +
//!   r·κ_q` (`r` = `Kapao_randomness`, `κ_q` = internal `Kapa`), `κ_dyn =
//!   κ₀` clamped to `[κ_oi, 0.99κ_c]` (upstream's nested comparisons); if
//!   `ε̂ > κ_oi`: `d₁ = 1 − κ_dyn/ε̂·((κ_c − ε̂)/(κ_c − κ_dyn))`; if `d₁ > d`:
//!   `d = std::min(d₁, 1)`, `number-damage += 1`, and at `d ≥ 1` the strain
//!   and rate are stored (`fulldam-valstrain(-rate)`); `σ *= 1 − d`.
//!
//! Parameters (parsable only): `Kapaoi` (1e-4), `Kapac` (2e-4), `Arate`
//! (0; the header documents 1, UD-782), `Brate` (1), `Crate` (1e-4, the
//! header's documented default, UD-578), `Kct` (1), `Kapao_randomness`
//! (0). With the defaults `κ₀ = Crate = Kapaoi` at rest: damage starts at
//! `ε̂ = Kapaoi` and reaches 1 at `Kapac` (the header's "`Kapaoi ≥ Crate`"
//! keeps `κ₀ ≤ κ_oi` for slow loading). Internals (registration order): `Kapa` (random field without a
//! parameter: draws its default `uniform [0, 0]`), `strain-rate-vreepeerlings`
//! (`dim²`), `fulldam-valstrain`, `fulldam-valstrain-rate`, `number-damage`,
//! `equi-strain`, `equi-strain-rate`. The time step argument of the kernel
//! only feeds a commented-out delayed-damage model and is not used.
//!
//! # Upstream defects
//!
//! * **UD-578** (A/B): `registerParam("Crate", Brate, 1.)` binds the key
//!   `Crate` to `Brate` and leaves `Crate` uninitialised;
//!   [`ExtraLawOptions::vreepeerlings_own_crate`] (default) registers
//!   `Crate` on its own member with the documented default `0.0001`; the
//!   compat mode binds the key to `Brate` and defines the member as 1 (a
//!   choice for the uninitialised value). The switch acts at parse time:
//!   select it at registration (`register_extra_materials_with`).
//! * **UD-781** (C, preserved): the rate `t1·|İ1|/(2k) + …` is not the time
//!   derivative of the coded `ε̂` for `k ≠ 1` (`|İ1|` loses the sign of
//!   `İ1`, where `d(t1·I1/(2k))/dt = t1·İ1/(2k)`).
//! * **UD-782** (C, preserved): the header documents `Arate` = 1, the code
//!   registers 0 (the code's value is kept).
//! * **UD-579** (B): when the point is not loading upstream throws "This
//!   instruction here was commented but has to be checked" (e.g. at rest);
//!   [`ExtraLawOptions::vreepeerlings_keep_rate_when_unloading`] keeps the
//!   stored rate (the commented-out self-assignment).
//! * **UD-580** (B, plus A for the 2-D out-of-bounds reads of the `is3D`
//!   terms): `J̇2` — 1-D subtracts `I1İ1/6`, n-D shear terms pair `∇u_ij`
//!   with `∇v_ji`; [`ExtraLawOptions::vreepeerlings_exact_j2_rate`] uses the
//!   exact rate of the coded `J2` (identical for symmetric gradients).
//! * The plane-stress invariants are commented out upstream: plane-strain
//!   invariants in every 2-D case (C, preserved).

use akantu_core::glibc_libm::pow;
use akantu_core::random::RandomParameter;
use akantu_core::tensor::{Matrix, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{Array, ElementType, Error, GhostType, Real, Result};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{InternalFieldMut, InternalFieldRef, LawContext, LawSpec};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};
use akantu_solid::Material;
use akantu_solid::material::{MaterialBase, MaterialFactory, MaterialParams};
use akantu_solid::materials::damage::DamageFields;
use akantu_solid::materials::elastic::MaterialElastic;

use crate::kernels::{check_lengths, scale_by_integrity, std_min};
use crate::options::ExtraLawOptions;

/// `vreepeerlings` (upstream `MaterialVreePeerlings<DIM, MaterialElastic>`).
#[derive(Clone, Debug)]
pub struct MaterialVreePeerlings<const DIM: usize> {
    /// The elastic parent.
    pub elastic: MaterialElastic<DIM>,
    /// `damage` (history), `damage dissipated energy`, `integral of sigma`.
    pub damage: DamageFields,
    /// `Kapa` (random internal field, no parameter upstream).
    pub kapa: InternalField<Real>,
    /// `strain-rate-vreepeerlings` (`dim²`).
    pub strain_rate: InternalField<Real>,
    /// `fulldam-valstrain`.
    pub full_dam_value_strain: InternalField<Real>,
    /// `fulldam-valstrain-rate`.
    pub full_dam_value_strain_rate: InternalField<Real>,
    /// `number-damage`.
    pub number_damage: InternalField<Real>,
    /// `equi-strain`.
    pub equi_strain: InternalField<Real>,
    /// `equi-strain-rate`.
    pub equi_strain_rate: InternalField<Real>,
    /// `Kapaoi`: initial threshold.
    pub kapaoi: Real,
    /// `Kapac`: final threshold.
    pub kapac: Real,
    /// `Arate`.
    pub arate: Real,
    /// `Brate`.
    pub brate: Real,
    /// `Crate`.
    pub crate_: Real,
    /// `Kct`: ratio between compressive and tensile strength.
    pub kct: Real,
    /// `Kapao_randomness`.
    pub kapao_randomness: Real,
    /// Extra-materials defect switches (UD-578…580).
    pub extra_options: ExtraLawOptions,
}

/// The default of `Crate` (header of `material_vreepeerlings.hh`: "`Crate`
/// : (default: 0.0001)", UD-578).
pub const DEFAULT_CRATE: Real = 0.0001;

/// The value of the uninitialised member `Crate` in the UD-578 compat mode
/// (a definition: upstream reads an uninitialised `Real`).
pub const COMPAT_CRATE: Real = 1.;

/// Invariants of one point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Invariants {
    /// `I1`.
    pub i1: Real,
    /// `J2`.
    pub j2: Real,
    /// `İ1`.
    pub i1_rate: Real,
    /// `J̇2`.
    pub j2_rate: Real,
}

impl<const DIM: usize> MaterialVreePeerlings<DIM> {
    /// New law (upstream constructor chain).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        Self::with_type(spec, mesh, "vreepeerlings")
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
            kapa: InternalField::new_random("Kapa", 1, RandomParameter::default())?,
            strain_rate: InternalField::new("strain-rate-vreepeerlings", DIM * DIM, 0.0)?,
            full_dam_value_strain: InternalField::new("fulldam-valstrain", 1, 0.0)?,
            full_dam_value_strain_rate: InternalField::new("fulldam-valstrain-rate", 1, 0.0)?,
            number_damage: InternalField::new("number-damage", 1, 0.0)?,
            equi_strain: InternalField::new("equi-strain", 1, 0.0)?,
            equi_strain_rate: InternalField::new("equi-strain-rate", 1, 0.0)?,
            kapaoi: 0.0001,
            kapac: 0.0002,
            arate: 0.,
            brate: 1.,
            crate_: DEFAULT_CRATE,
            kct: 1.,
            kapao_randomness: 0.,
            extra_options: ExtraLawOptions::default(),
        })
    }

    /// The invariants of `∇u` and their rates from `∇v` (module docs,
    /// UD-580).
    #[must_use]
    pub fn invariants(&self, g: &Matrix<DIM, DIM>, v: &Matrix<DIM, DIM>) -> Invariants {
        let exact = self.extra_options.vreepeerlings_exact_j2_rate;
        let at = |m: &Matrix<DIM, DIM>, i: usize, j: usize| m[j][i];
        let i1 = g.trace();
        let i1_rate = v.trace();
        let mut j2 = 0.;
        let j2_rate = if DIM == 1 {
            j2 = 0.5 * at(g, 0, 0) * at(g, 0, 0) - i1 * i1 / 6.;
            let div = if exact { 3. } else { 6. };
            at(g, 0, 0) * at(v, 0, 0) - i1 * i1_rate / div
        } else {
            for i in 0..DIM {
                for j in i..DIM {
                    let x = if i == j {
                        // (i == j)·g_ii² + 0.5·(1 − (i == j))·(…) with (…) = 4 g_ii²
                        at(g, i, i) * at(g, i, i) + 0.5 * 0. * 0.
                    } else {
                        let s = at(g, i, j) + at(g, j, i);
                        0. + 0.5 * 1. * (s * s)
                    };
                    j2 += 0.5 * x;
                }
            }
            j2 -= i1 * i1 / 6.;
            // the three shear pairs (0,1), (1,2), (2,0) of the upstream sum,
            // each added twice
            let shear = |i: usize, j: usize| {
                if exact {
                    (0.5 * (at(g, i, j) + at(g, j, i))) * (0.5 * (at(v, i, j) + at(v, j, i)))
                } else {
                    at(g, i, j) * at(v, j, i)
                }
            };
            let mut s = at(g, 0, 0) * at(v, 0, 0) + at(g, 1, 1) * at(v, 1, 1);
            if DIM == 3 {
                s += at(g, 2, 2) * at(v, 2, 2);
            }
            s += shear(0, 1);
            s += shear(0, 1);
            if DIM == 3 {
                s += shear(1, 2);
                s += shear(1, 2);
                s += shear(2, 0);
                s += shear(2, 0);
            }
            s - i1 * i1_rate / 3.
        };
        Invariants {
            i1,
            j2,
            i1_rate,
            j2_rate,
        }
    }

    /// The equivalent strain and its rate (`previous_rate`: the stored rate,
    /// kept when the point is not loading, UD-579).
    ///
    /// # Errors
    /// The upstream error when not loading in compat mode.
    pub fn equivalent_strain(&self, inv: &Invariants, previous_rate: Real) -> Result<(Real, Real)> {
        let nu = self.elastic.thermal.nu;
        let k = self.kct;
        let tmp_1 = (k - 1.) / (1. - 2. * nu);
        let tmp_2 = (12. * k) / ((1. + nu) * (1. + nu));
        let root = (tmp_1 * tmp_1 * inv.i1 * inv.i1 + tmp_2 * inv.j2).sqrt();
        let strain = tmp_1 * inv.i1 / (2. * k) + 1. / (2. * k) * root;
        let rate = if inv.i1 * inv.i1_rate > 0. || inv.j2_rate > 0. {
            tmp_1 * inv.i1_rate.abs() / (2. * k)
                + 1. / (4. * k) * (2. * tmp_1 * tmp_1 * inv.i1 * inv.i1_rate + tmp_2 * inv.j2_rate)
                    / root
        } else if self.extra_options.vreepeerlings_keep_rate_when_unloading {
            previous_rate
        } else {
            return Err(Error::invalid(
                "This instruction here was commented but has to be checked",
            ));
        };
        Ok((strain, rate))
    }

    /// Upstream `computeDamageAndStressOnQuad` (without the stress scaling):
    /// the new `(d, number-damage, fulldam-valstrain, fulldam-valstrain-rate)`.
    #[must_use]
    pub fn damage_on_quad(
        &self,
        damage: Real,
        strain: Real,
        rate: Real,
        kapa_q: Real,
        state: (Real, Real, Real),
    ) -> (Real, Real, Real, Real) {
        let (mut nb, mut full_strain, mut full_rate) = state;
        let mut dam = damage;
        let kapao = self.crate_
            * (1. + pow(rate.abs() * self.arate, self.brate))
            * (1. - self.kapao_randomness)
            + self.kapao_randomness * kapa_q;
        let kapac_99 = self.kapac * 0.99;
        let kapaodyn = if kapao > self.kapaoi {
            if kapao < kapac_99 { kapao } else { kapac_99 }
        } else {
            self.kapaoi
        };
        let fd1 = strain - self.kapaoi;
        if fd1 > 0. {
            let dam1 = 1. - kapaodyn / strain * ((self.kapac - strain) / (self.kapac - kapaodyn));
            if dam1 > dam {
                dam = std_min(dam1, 1.);
                nb += 1.;
                if dam >= 1. {
                    full_strain = strain;
                    full_rate = rate;
                }
            }
        }
        (dam, nb, full_strain, full_rate)
    }

    /// Velocity gradient of the block into `strain-rate-vreepeerlings`.
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
            "MaterialVreePeerlings::computeStress: strain rate",
            out.len(),
            &[gv.len()],
        )?;
        if !gv.is_empty() {
            out.copy_from(&gv)?;
        }
        Ok(())
    }

    /// The point loop of upstream `computeStress` (after the elastic stress
    /// and the strain rate).
    ///
    /// # Errors
    /// Missing blocks, size mismatches or the UD-579 compat error.
    pub fn stress_loop(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        let n = DIM * DIM;
        let non_local = self.elastic.base().is_non_local;
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let grad_v = self.strain_rate.get(ty, ghost)?.clone();
        let mut sigma = self.elastic.base().stress.get(ty, ghost)?.clone();
        let mut dam = self.damage.damage.get(ty, ghost)?.clone();
        let kapa = self.kapa.get(ty, ghost)?.clone();
        let mut es = self.equi_strain.get(ty, ghost)?.clone();
        let mut esr = self.equi_strain_rate.get(ty, ghost)?.clone();
        let mut nb = self.number_damage.get(ty, ghost)?.clone();
        let mut fs = self.full_dam_value_strain.get(ty, ghost)?.clone();
        let mut fsr = self.full_dam_value_strain_rate.get(ty, ghost)?.clone();
        let nq = grad_u.len();
        check_lengths(
            "MaterialVreePeerlings::computeStress",
            nq,
            &[
                grad_v.len(),
                sigma.len(),
                dam.len(),
                kapa.len(),
                es.len(),
                esr.len(),
                nb.len(),
                fs.len(),
                fsr.len(),
            ],
        )?;
        for (q, ((g, v), s)) in grad_u
            .as_slice()
            .chunks_exact(n)
            .zip(grad_v.as_slice().chunks_exact(n))
            .zip(sigma.as_mut_slice().chunks_exact_mut(n))
            .enumerate()
        {
            let inv = self.invariants(as_matrix(g)?, as_matrix(v)?);
            let (strain, rate) = self.equivalent_strain(&inv, esr.as_slice()[q])?;
            es.as_mut_slice()[q] = strain;
            esr.as_mut_slice()[q] = rate;
            if !non_local {
                let (d, n_d, f_s, f_r) = self.damage_on_quad(
                    dam.as_slice()[q],
                    strain,
                    rate,
                    kapa.as_slice()[q],
                    (nb.as_slice()[q], fs.as_slice()[q], fsr.as_slice()[q]),
                );
                dam.as_mut_slice()[q] = d;
                nb.as_mut_slice()[q] = n_d;
                fs.as_mut_slice()[q] = f_s;
                fsr.as_mut_slice()[q] = f_r;
                scale_by_integrity(as_matrix_mut::<DIM, DIM>(s)?, d);
            }
        }
        let base = self.elastic.base_mut();
        base.stress.get_mut(ty, ghost)?.copy_from(&sigma)?;
        self.damage.damage.get_mut(ty, ghost)?.copy_from(&dam)?;
        self.equi_strain.get_mut(ty, ghost)?.copy_from(&es)?;
        self.equi_strain_rate.get_mut(ty, ghost)?.copy_from(&esr)?;
        self.number_damage.get_mut(ty, ghost)?.copy_from(&nb)?;
        self.full_dam_value_strain
            .get_mut(ty, ghost)?
            .copy_from(&fs)?;
        self.full_dam_value_strain_rate
            .get_mut(ty, ghost)?
            .copy_from(&fsr)?;
        Ok(())
    }

    pub(crate) fn visit_own(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        f("Kapa", InternalFieldRef::Real(&self.kapa));
        f(
            "strain-rate-vreepeerlings",
            InternalFieldRef::Real(&self.strain_rate),
        );
        f(
            "fulldam-valstrain",
            InternalFieldRef::Real(&self.full_dam_value_strain),
        );
        f(
            "fulldam-valstrain-rate",
            InternalFieldRef::Real(&self.full_dam_value_strain_rate),
        );
        f("number-damage", InternalFieldRef::Real(&self.number_damage));
        f("equi-strain", InternalFieldRef::Real(&self.equi_strain));
        f(
            "equi-strain-rate",
            InternalFieldRef::Real(&self.equi_strain_rate),
        );
    }

    pub(crate) fn visit_own_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        f("Kapa", InternalFieldMut::Real(&mut self.kapa))?;
        f(
            "strain-rate-vreepeerlings",
            InternalFieldMut::Real(&mut self.strain_rate),
        )?;
        f(
            "fulldam-valstrain",
            InternalFieldMut::Real(&mut self.full_dam_value_strain),
        )?;
        f(
            "fulldam-valstrain-rate",
            InternalFieldMut::Real(&mut self.full_dam_value_strain_rate),
        )?;
        f(
            "number-damage",
            InternalFieldMut::Real(&mut self.number_damage),
        )?;
        f("equi-strain", InternalFieldMut::Real(&mut self.equi_strain))?;
        f(
            "equi-strain-rate",
            InternalFieldMut::Real(&mut self.equi_strain_rate),
        )
    }

    pub(crate) fn after_init(&mut self) -> Result<()> {
        Ok(())
    }

    /// Upstream `computeStress(type, ghost)`.
    pub(crate) fn stress_impl(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        // MaterialDamage::computeStress = MaterialElastic::computeStress
        self.elastic.compute_stress(ctx, ty, ghost)?;
        self.update_strain_rate(ctx, ty, ghost)?;
        self.stress_loop(ty, ghost)
    }
}

macro_rules! vreepeerlings_params {
    ($($ps:ident)?) => {
        akantu_solid::elastic_parent_params!($($ps)? Self, [elastic], [
            akantu_solid::real_param!(Self, "Kapaoi", ParameterAccess::PARSABLE, "", kapaoi),
            akantu_solid::real_param!(Self, "Kapac", ParameterAccess::PARSABLE, "", kapac),
            akantu_solid::real_param!(Self, "Arate", ParameterAccess::PARSABLE, "", arate),
            akantu_solid::real_param!(Self, "Brate", ParameterAccess::PARSABLE, "", brate),
            ParamDescriptor::<Self>::new(
                "Crate",
                ::akantu_parser::param::ParamKind::Real,
                ParameterAccess::PARSABLE,
                "",
                |s| ::akantu_parser::param::ParamValue::Real(
                    if s.extra_options.vreepeerlings_own_crate { s.crate_ } else { s.brate },
                ),
                Some(|s, v| {
                    // UD-578: upstream binds the key `Crate` to `Brate`
                    let x = v.as_real()?;
                    if s.extra_options.vreepeerlings_own_crate {
                        s.crate_ = x;
                    } else {
                        s.brate = x;
                    }
                    Ok(())
                }),
            ),
            akantu_solid::real_param!(Self, "Kct", ParameterAccess::PARSABLE, "", kct),
            akantu_solid::real_param!(Self, "Kapao_randomness", ParameterAccess::PARSABLE, "", kapao_randomness),
        ])
    };
}

impl<const DIM: usize> MaterialVreePeerlings<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = vreepeerlings_params!();
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = vreepeerlings_params!(plane_stress);
}

impl<const DIM: usize> MaterialParams for MaterialVreePeerlings<DIM> {
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

crate::damage_law::impl_damage_law!(MaterialVreePeerlings);

impl<const DIM: usize> crate::options::HasExtraOptions for MaterialVreePeerlings<DIM> {
    fn extra_options(&self) -> &ExtraLawOptions {
        &self.extra_options
    }

    /// Also resets `Crate` to the default of the mode (UD-578): the
    /// documented `0.0001` when the key is bound to `Crate`, the defined
    /// value 1 of the uninitialised member otherwise.
    fn set_extra_options(&mut self, options: ExtraLawOptions) {
        self.extra_options = options;
        self.crate_ = if options.vreepeerlings_own_crate {
            DEFAULT_CRATE
        } else {
            COMPAT_CRATE
        };
    }
}

/// Register `vreepeerlings` (legacy-list keyword; never registered
/// upstream, UD-572).
///
/// # Errors
/// A duplicate registration.
pub fn register(factory: &mut MaterialFactory, options: ExtraLawOptions) -> Result<()> {
    crate::damage_law::register_law!(factory, "vreepeerlings", MaterialVreePeerlings, options)
}

#[cfg(test)]
mod tests {
    #[test]
    fn material_trait_is_object_safe_for_the_law() {
        fn takes(_: &dyn super::Material) {}
        let _ = takes;
    }
}
