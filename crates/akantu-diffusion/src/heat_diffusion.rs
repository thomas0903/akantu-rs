// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion_model/heat_diffusion.hh,
// heat_diffusion.cc.

//! The `heat_diffusion` law (DIFF-009): Fourier conduction with an optional
//! linear temperature dependence of the conductivity.
//!
//! Parameters (upstream registration order, all parsable and modifiable):
//! `density`, `conductivity` (a matrix; the top-left `dim × dim` block is
//! kept by `updateInternalParameters`), `conductivity_variation` (`0`),
//! `temperature_reference` (`0`), `capacity`; plus the constitutive law's
//! `name`.
//!
//! * diffusivity at a quadrature point: `C = conductivity`, then
//!   `C_ij += conductivity_variation · (T − T_ref)` for **every** entry,
//!   off-diagonal ones included (UD-282, category C: reproduced);
//! * `D∇u = C · ∇u` (Eigen's coefficient-based fixed-size product: each
//!   component a halving-tree reduction over `dim`, D-008);
//! * capacity density `ρc = density · capacity`;
//! * stable time step `2 h² / 4 · ρ c / k_max` (`k_max` the largest real
//!   part of the eigenvalues of `conductivity`, at least `0`);
//! * energy `"thermal"`: `∫ c ρ T` over the law's not-ghost elements, or
//!   one element.

use akantu_core::eigen_order;
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, Element, ElementType, Error, GhostType, Real, Result};
use akantu_linalg::dense::{Storage, eig};
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawSpec, ModelServices,
};
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, fit_matrix, table_get,
    table_info, table_names, table_set, table_set_from_input,
};
use akantu_parser::{ParameterRegistry, Parsable, ParserType};

use crate::law::{DiffusionLaw, DiffusionLawBase, DiffusionLawContext, store_block};

/// Registered name of the law.
pub const HEAT_DIFFUSION: &str = "heat_diffusion";

/// Upstream `HeatDiffusion<dim>`.
#[derive(Debug)]
pub struct HeatDiffusion {
    base: DiffusionLawBase,
    /// Density `ρ`.
    pub density: Real,
    /// Capacity `c`.
    pub capacity: Real,
    /// Conductivity matrix (as parsed / set; `dim × dim` after
    /// `updateInternalParameters`).
    pub conductivity: MatrixX,
    /// Linear variation of the conductivity with the temperature.
    pub conductivity_variation: Real,
    /// Reference temperature of the variation.
    pub t_ref: Real,
}

impl HeatDiffusion {
    /// Upstream allocator `HeatDiffusion<dim>(model, id)`.
    pub fn new(spec: &LawSpec, services: &dyn ModelServices) -> Result<Self> {
        let base = DiffusionLawBase::new(
            &spec.id,
            HEAT_DIFFUSION,
            services.mesh(),
            spec.spatial_dimension,
        )?;
        Ok(Self {
            base,
            density: 0.0,
            capacity: 0.0,
            conductivity: MatrixX::zeros(0, 0),
            conductivity_variation: 0.0,
            t_ref: 0.0,
        })
    }

    /// Upstream `getThermalEnergy` integrand: `c ρ T` per quadrature point
    /// (`capacity * density * T`, left to right).
    fn thermal_integrand(&self, t: &[Real]) -> Vec<Real> {
        t.iter()
            .map(|&ti| self.capacity * self.density * ti)
            .collect()
    }
}

/// Upstream `ConstitutiveLaw::name` and the `HeatDiffusion` parameters, in
/// registration order.
const PARAMS: &[ParamDescriptor<HeatDiffusion>] = &[
    ParamDescriptor::new(
        "name",
        ParamKind::ByteString,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "Name of the constitutive law",
        |s| ParamValue::ByteString(s.base.name_bytes.clone()),
        Some(|s, v| {
            s.base.set_name(v.into_byte_string()?);
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "density",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "",
        |s| ParamValue::Real(s.density),
        Some(|s, v| {
            s.density = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "conductivity",
        ParamKind::MatrixReal,
        ParameterAccess::PARSMOD,
        "",
        |s| ParamValue::MatrixReal(s.conductivity.clone()),
        Some(|s, v| match v {
            ParamValue::MatrixReal(m) => {
                s.conductivity = m;
                Ok(())
            }
            other => Err(Error::invalid(format!(
                "parameter conductivity expects a matrix, got {:?}",
                other.kind()
            ))),
        }),
    ),
    ParamDescriptor::new(
        "conductivity_variation",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "",
        |s| ParamValue::Real(s.conductivity_variation),
        Some(|s, v| {
            s.conductivity_variation = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "temperature_reference",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "",
        |s| ParamValue::Real(s.t_ref),
        Some(|s, v| {
            s.t_ref = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "capacity",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "",
        |s| ParamValue::Real(s.capacity),
        Some(|s, v| {
            s.capacity = v.as_real()?;
            Ok(())
        }),
    ),
];

impl ParameterRegistry for HeatDiffusion {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        table_get(PARAMS, self, name)
    }

    /// Upstream `ParameterRegistry::set`: the value, then
    /// `updateInternalParameters()`.
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set(PARAMS, self, name, value)?;
        self.update_internal_parameters()
    }

    fn param_names(&self) -> Vec<&'static str> {
        table_names(PARAMS)
    }

    /// Upstream `HeatDiffusion::updateInternalParameters`: keep the
    /// top-left `dim × dim` block of `conductivity`, then
    /// `diffusivity.set(conductivity)` (every existing quadrature point of
    /// both ghost types; the default value of new entries stays `0`).
    ///
    /// # Errors
    /// A conductivity smaller than `dim × dim` (upstream's fixed-size
    /// `block<dim, dim>` reads out of bounds, UD-285).
    fn update_internal_parameters(&mut self) -> Result<()> {
        let d = self.base.spatial_dimension;
        if self.conductivity.rows() < d || self.conductivity.cols() < d {
            return Err(Error::DimensionMismatch {
                context: "heat_diffusion: conductivity must be at least dim x dim (UD-285)",
                expected: d * d,
                got: self.conductivity.rows() * self.conductivity.cols(),
            });
        }
        let mut c = MatrixX::zeros(d, d);
        for j in 0..d {
            for i in 0..d {
                *c.at_mut(i, j) = self.conductivity.at(i, j);
            }
        }
        self.conductivity = c;
        let src = self.conductivity.as_slice().to_vec();
        for ghost in GhostType::ALL {
            for (_, a) in self
                .base
                .diffusivity
                .values_mut()
                .as_map_mut()
                .iter_mut(ghost)
            {
                for t in a.write()?.as_mut_slice().chunks_exact_mut(d * d) {
                    t.copy_from_slice(&src);
                }
            }
        }
        Ok(())
    }

    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        table_info(PARAMS, name)
    }

    /// Upstream `setAuto`: a matrix parameter that already has a size keeps
    /// the top-left block of the parsed matrix.
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        let value = match (name, value) {
            ("conductivity", ParamValue::MatrixReal(m)) => {
                let (r, c) = (self.conductivity.rows(), self.conductivity.cols());
                ParamValue::MatrixReal(fit_matrix(r, c, m)?)
            }
            (_, v) => v,
        };
        table_set_from_input(PARAMS, self, name, value)
    }
}

impl Parsable for HeatDiffusion {
    fn section_type(&self) -> ParserType {
        ParserType::ConstitutiveLaw
    }

    fn parsable_id(&self) -> &str {
        &self.base.id
    }
}

impl ConstitutiveLaw for HeatDiffusion {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn law_id(&self) -> &str {
        &self.base.id
    }

    fn law_type(&self) -> &str {
        &self.base.law_type
    }

    fn element_filter(&self) -> &akantu_core::ElementTypeMapArray<akantu_core::Idx> {
        &self.base.element_filter
    }

    fn element_filter_mut(&mut self) -> &mut akantu_core::ElementTypeMapArray<akantu_core::Idx> {
        &mut self.base.element_filter
    }

    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.base.for_each_internal(f);
    }

    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.base.for_each_internal_mut(f)
    }
}

impl DiffusionLaw for HeatDiffusion {
    fn base(&self) -> &DiffusionLawBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut DiffusionLawBase {
        &mut self.base
    }

    /// Upstream `computeDiffusivityOnQuadPoints(type, ghost)`: skipped when
    /// the diffusivity release of `(type, ghost)` is not `-1` and equals the
    /// model's release; otherwise **every** type of the filter of `ghost`
    /// is recomputed (upstream's inner loop shadows `type`) and only the
    /// release of `(type, ghost)` is updated (UD-284, category C).
    fn compute_diffusivity_on_quad_points(
        &mut self,
        ctx: &DiffusionLawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let rel = self.base.diffusivity_release(ty, ghost);
        if rel != -1 && rel == ctx.release {
            return Ok(());
        }
        let d = self.base.spatial_dimension;
        let fe = ctx.fe()?;
        let cond = self.conductivity.as_slice().to_vec();
        if cond.len() != d * d {
            return Err(Error::invalid(
                "heat_diffusion: conductivity not initialised (updateInternalParameters)",
            ));
        }
        for t in self.base.filter_types(ghost) {
            let filter = self.base.filter(t, ghost).to_vec();
            let mut tq = Array::new(1);
            fe.interpolate_on_integration_points(
                ctx.mesh(),
                ctx.diffusion.view(),
                &mut tq,
                1,
                t,
                ghost,
                Some(&filter),
            )?;
            // `conductivity_variation * (T - T_ref)` is one scalar per
            // quadrature point, added to every entry of C
            let var = self.conductivity_variation;
            let t_ref = self.t_ref;
            let mut block = self.base.diffusivity.get_mut(t, ghost)?;
            if block.len() != tq.len() {
                return Err(Error::DimensionMismatch {
                    context: "computeDiffusivityOnQuadPoints: diffusivity vs temperature",
                    expected: block.len(),
                    got: tq.len(),
                });
            }
            for (c, &tt) in block
                .as_mut_slice()
                .chunks_exact_mut(d * d)
                .zip(tq.as_slice())
            {
                let s = var * (tt - t_ref);
                for (ci, &k) in c.iter_mut().zip(&cond) {
                    *ci = k + s;
                }
            }
        }
        self.base
            .diffusivity_release
            .insert((ty, ghost), ctx.release);
        Ok(())
    }

    /// Upstream `computeDiffusivityGradUOnQuadPoints(type, ghost)`: the
    /// diffusivity, then `D∇u = C · ∇u` per quadrature point.
    fn compute_diffusivity_grad_u_on_quad_points(
        &mut self,
        ctx: &DiffusionLawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.compute_diffusivity_on_quad_points(ctx, ty, ghost)?;
        let d = self.base.spatial_dimension;
        let grad = self.base.grad_u.get(ty, ghost)?.as_slice().to_vec();
        let diff = self.base.diffusivity.get(ty, ghost)?.as_slice().to_vec();
        let nq = grad.len() / d.max(1);
        if diff.len() != nq * d * d {
            return Err(Error::DimensionMismatch {
                context: "computeDiffusivityGradUOnQuadPoints: diffusivity vs gradient",
                expected: nq * d * d,
                got: diff.len(),
            });
        }
        let mut out = Array::try_zeros(nq, d)?;
        for ((k_bt, bt), c) in out
            .as_mut_slice()
            .chunks_exact_mut(d)
            .zip(grad.chunks_exact(d))
            .zip(diff.chunks_exact(d * d))
        {
            // fixed-size `Matrix<dim,dim> * Vector<dim>`: coefficient-based,
            // halving tree over the compile-time depth (left operand fixed)
            for (i, ki) in k_bt.iter_mut().enumerate() {
                *ki = eigen_order::product_coeff(d, true, &|k| c[i + k * d] * bt[k]);
            }
        }
        store_block(&mut self.base.d_grad_u, ty, ghost, out)
    }

    /// Upstream `getStableTimeStep(h)`: `2. * h * h / 4. * density *
    /// capacity / k_max` with `k_max = max(0, max_i Re λ_i(conductivity))`
    /// (`conductivity.eig(ce)`, a dynamic matrix: Eigen's `EigenSolver`).
    fn stable_time_step(&self, element_size: Real) -> Result<Real> {
        let ce = eig(&self.conductivity, Storage::Dynamic)?;
        let mut conductivity_max: Real = 0.0;
        for c in ce {
            // std::max(c, conductivity_max)
            conductivity_max = if c < conductivity_max {
                conductivity_max
            } else {
                c
            };
        }
        let h = element_size;
        Ok(2. * h * h / 4. * self.density * self.capacity / conductivity_max)
    }

    /// Upstream `getRho()`: `density * capacity`.
    fn rho(&self) -> Real {
        self.density * self.capacity
    }

    /// Upstream `getEnergy(id)`: `"thermal"` → `Σ_type ∫ c ρ T` over the
    /// law's not-ghost elements (per type: integral of the filter's
    /// elements, `Math::reduce`). Upstream integrates without the element
    /// filter (`heat_diffusion.cc:145`): out-of-bounds / uninitialised
    /// reads when the law does not own every element of a type (UD-278,
    /// category A); the port integrates over the filter (identical for a
    /// law owning every element of its types in mesh order). Other ids: `0`.
    fn law_energy(&self, ctx: &DiffusionLawContext<'_>, energy_id: &str) -> Result<Real> {
        if energy_id != "thermal" {
            return Ok(0.0);
        }
        let fe = ctx.fe()?;
        let mut eth = 0.0;
        for ty in self.base.filter_types(GhostType::NotGhost) {
            let filter = self.base.filter(ty, GhostType::NotGhost).to_vec();
            let mut tq = Array::new(1);
            fe.interpolate_on_integration_points(
                ctx.mesh(),
                ctx.diffusion.view(),
                &mut tq,
                1,
                ty,
                GhostType::NotGhost,
                Some(&filter),
            )?;
            let e = self.thermal_integrand(tq.as_slice());
            eth += fe.integrate_scalar(
                akantu_core::array::ArrayView::new(&e, 1)?,
                ty,
                GhostType::NotGhost,
                Some(&filter),
            )?;
        }
        Ok(eth)
    }

    /// Upstream `getEnergy(id, element)`: `"thermal"` → `∫_e c ρ T` (the
    /// element's temperature interpolated at its quadrature points). With
    /// `compat.element_energy_uses_law_local_index` the law-local index is
    /// used as the mesh element, as upstream (UD-280). Upstream's
    /// per-element integral writes `nb_quadrature_points` values into one
    /// `Real` (UD-277, category A); its value is the dot product of the
    /// integrand with the Jacobians, which the port computes.
    fn law_energy_of_element(
        &self,
        ctx: &DiffusionLawContext<'_>,
        energy_id: &str,
        local: Element,
        element: Element,
    ) -> Result<Real> {
        if energy_id != "thermal" {
            return Ok(0.0);
        }
        let target = if ctx.compat.element_energy_uses_law_local_index {
            local
        } else {
            element
        };
        let fe = ctx.fe()?;
        let filter = [target.index()];
        let mut tq = Array::new(1);
        fe.interpolate_on_integration_points(
            ctx.mesh(),
            ctx.diffusion.view(),
            &mut tq,
            1,
            target.ty(),
            target.ghost_type(),
            Some(&filter),
        )?;
        let e = self.thermal_integrand(tq.as_slice());
        fe.integrate_scalar(
            akantu_core::array::ArrayView::new(&e, 1)?,
            target.ty(),
            target.ghost_type(),
            Some(&filter),
        )
    }
}
