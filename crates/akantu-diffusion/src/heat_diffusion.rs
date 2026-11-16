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

use crate::law::{DiffusionLaw, DiffusionLawBase, DiffusionLawContext};

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
    /// The temperature at the quadrature points of the last computed
    /// block (kept between calls: no allocation per step).
    tq_scratch: Array<Real>,
}

impl HeatDiffusion {
    /// [`DiffusionLaw::compute_diffusivity_on_quad_points`]; `true` when
    /// this call wrote the conductivity itself into every point of the
    /// block `(ty, ghost)` (the temperature term vanishes).
    fn diffusivity_on_quad_points(
        &mut self,
        ctx: &DiffusionLawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<bool> {
        let rel = self.base.diffusivity_release(ty, ghost);
        if rel != -1 && rel == ctx.release {
            return Ok(false);
        }
        let d = self.base.spatial_dimension;
        let fe = ctx.fe()?;
        let cond = self.conductivity.as_slice();
        if cond.len() != d * d {
            return Err(Error::invalid(
                "heat_diffusion: conductivity not initialised (updateInternalParameters)",
            ));
        }
        let var = self.conductivity_variation;
        let t_ref = self.t_ref;
        let constant = temperature_term_vanishes(var, t_ref, cond, ctx.diffusion.as_slice());
        let base = &mut self.base;
        let mut constant_ty = false;
        for t in base.filter_types(ghost) {
            let filter = base
                .element_filter
                .get(t, ghost)
                .map(Array::as_slice)
                .unwrap_or(&[]);
            if let (true, Ok(nq)) = (constant, fe.nb_integration_points(t, ghost)) {
                // every entry is `k + (±0)`, which is `k` (see
                // `temperature_term_vanishes`): no interpolation of `T`
                let mut block = base.diffusivity.get_mut(t, ghost)?;
                if block.len() != filter.len() * nq {
                    return Err(Error::DimensionMismatch {
                        context: "computeDiffusivityOnQuadPoints: diffusivity vs temperature",
                        expected: block.len(),
                        got: filter.len() * nq,
                    });
                }
                for c in block.as_mut_slice().chunks_exact_mut(d * d) {
                    c.copy_from_slice(cond);
                }
                constant_ty |= t == ty;
                continue;
            }
            let tq = &mut self.tq_scratch;
            fe.interpolate_on_integration_points(
                ctx.mesh(),
                ctx.diffusion.view(),
                tq,
                1,
                t,
                ghost,
                Some(filter),
            )?;
            // `conductivity_variation * (T - T_ref)` is one scalar per
            // quadrature point, added to every entry of C
            let mut block = base.diffusivity.get_mut(t, ghost)?;
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
                for (ci, &k) in c.iter_mut().zip(cond) {
                    *ci = k + s;
                }
            }
        }
        base.diffusivity_release.insert((ty, ghost), ctx.release);
        Ok(constant_ty)
    }

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
            tq_scratch: Array::new(1),
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

/// Largest magnitude of a nodal or reference temperature for which
/// `T_q − T_ref` is finite (`2^960`): an interpolated `T_q` is a sum of at
/// most a few dozen products of nodal values with shape functions of
/// magnitude below `2^4` at the quadrature points, so it stays below
/// `2^970` and the difference below `2^971`.
const BOUNDED_TEMPERATURE: Real = Real::from_bits(0x7bf0_0000_0000_0000);

/// Whether every diffusivity entry `k + var·(T_q − T_ref)` equals `k` bit
/// for bit whatever the quadrature-point temperatures, so that the
/// interpolation of `T` can be skipped: `var` is `±0`, `T_ref` and every
/// nodal temperature are finite and bounded ([`BOUNDED_TEMPERATURE`]),
/// hence `T_q − T_ref` is finite and `var·(T_q − T_ref)` is `±0`; and no
/// entry `k` is a NaN or `−0` (`−0 + +0 = +0`), so `k + (±0) = k`.
/// Otherwise the temperature term is computed as before.
fn temperature_term_vanishes(var: Real, t_ref: Real, cond: &[Real], nodal_t: &[Real]) -> bool {
    var == 0.0
        && t_ref.abs() <= BOUNDED_TEMPERATURE
        && cond
            .iter()
            .all(|k| !k.is_nan() && !(*k == 0.0 && k.is_sign_negative()))
        && nodal_t.iter().all(|t| t.abs() <= BOUNDED_TEMPERATURE)
}

/// `D∇u` of every point for the dimension `D`: per component the
/// fixed-size `Matrix<D, D> * Vector<D>` coefficient (halving tree over the
/// compile-time depth, left operand fixed), as [`d_grad_u_general`].
#[inline(always)]
fn d_grad_u_fixed<const D: usize>(grad: &[Real], diff: &[Real], out: &mut [Real]) {
    for ((k_bt, bt), c) in out
        .chunks_exact_mut(D)
        .zip(grad.chunks_exact(D))
        .zip(diff.chunks_exact(D * D))
    {
        for (i, ki) in k_bt.iter_mut().enumerate() {
            *ki = eigen_order::product_coeff(D, true, &|k| c[i + k * D] * bt[k]);
        }
    }
}

/// [`d_grad_u_fixed`] with the same diffusivity `c` (`D × D`) at every
/// point.
#[inline(always)]
fn d_grad_u_constant<const D: usize>(grad: &[Real], c: &[Real], out: &mut [Real]) {
    let c = &c[..D * D];
    for (k_bt, bt) in out.chunks_exact_mut(D).zip(grad.chunks_exact(D)) {
        for (i, ki) in k_bt.iter_mut().enumerate() {
            *ki = eigen_order::product_coeff(D, true, &|k| c[i + k * D] * bt[k]);
        }
    }
}

/// `D∇u` of every point for a run-time dimension `d`.
fn d_grad_u_general(d: usize, grad: &[Real], diff: &[Real], out: &mut [Real]) {
    for ((k_bt, bt), c) in out
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
        self.diffusivity_on_quad_points(ctx, ty, ghost).map(|_| ())
    }

    /// Upstream `computeDiffusivityGradUOnQuadPoints(type, ghost)`: the
    /// diffusivity, then `D∇u = C · ∇u` per quadrature point, written into
    /// the `D∇u` block (resized to the gradient's points).
    fn compute_diffusivity_grad_u_on_quad_points(
        &mut self,
        ctx: &DiffusionLawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let constant = self.diffusivity_on_quad_points(ctx, ty, ghost)?;
        let d = self.base.spatial_dimension;
        let cond = self.conductivity.as_slice();
        let base = &mut self.base;
        let grad = base.grad_u.get(ty, ghost)?;
        let diff = base.diffusivity.get(ty, ghost)?;
        let nq = grad.as_slice().len() / d.max(1);
        if diff.as_slice().len() != nq * d * d {
            return Err(Error::DimensionMismatch {
                context: "computeDiffusivityGradUOnQuadPoints: diffusivity vs gradient",
                expected: nq * d * d,
                got: diff.as_slice().len(),
            });
        }
        base.d_grad_u.resize(ty, ghost, nq)?;
        let mut out = base.d_grad_u.get_mut(ty, ghost)?;
        let (g, c, o) = (grad.as_slice(), diff.as_slice(), out.as_mut_slice());
        if constant && cond.len() == d * d {
            // every point of the diffusivity block was just set to the
            // conductivity: the same factors, read from the `d × d` matrix
            // instead of from every point's copy
            match d {
                1 => d_grad_u_constant::<1>(g, cond, o),
                2 => d_grad_u_constant::<2>(g, cond, o),
                3 => d_grad_u_constant::<3>(g, cond, o),
                _ => d_grad_u_general(d, g, c, o),
            }
            return Ok(());
        }
        match d {
            1 => d_grad_u_fixed::<1>(g, c, o),
            2 => d_grad_u_fixed::<2>(g, c, o),
            3 => d_grad_u_fixed::<3>(g, c, o),
            _ => d_grad_u_general(d, g, c, o),
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::bitwise_testing::{Values, same};

    /// `D∇u` with the constant dimensions against the run-time dimension,
    /// bit for bit (D-046 rule; payload NaNs exact).
    #[test]
    fn fixed_dimension_d_grad_u_matches_the_general_kernel_bitwise() {
        let mut v = Values::new(0x4ea7_d6ad);
        for round in 0..20_000 {
            let payload = round % 5 == 4;
            for d in 1..=3 {
                let nq = 3;
                if payload {
                    let k = 1 + v.next_u64() % (nq * (d + d * d)) as u64;
                    v.payload_at(k);
                }
                let mut g = vec![0.0; nq * d];
                let mut c = vec![0.0; nq * d * d];
                v.fill(&mut g);
                v.fill(&mut c);
                v.default_mode();
                let (mut a, mut b) = (vec![0.0; nq * d], vec![0.0; nq * d]);
                match d {
                    1 => d_grad_u_fixed::<1>(&g, &c, &mut a),
                    2 => d_grad_u_fixed::<2>(&g, &c, &mut a),
                    _ => d_grad_u_fixed::<3>(&g, &c, &mut a),
                }
                d_grad_u_general(std::hint::black_box(d), &g, &c, &mut b);
                // one diffusivity at every point: the constant kernel
                let first: Vec<Real> = c[..d * d].to_vec();
                let rep: Vec<Real> = (0..nq).flat_map(|_| first.iter().copied()).collect();
                let (mut e, mut f) = (vec![0.0; nq * d], vec![0.0; nq * d]);
                match d {
                    1 => d_grad_u_constant::<1>(&g, &first, &mut e),
                    2 => d_grad_u_constant::<2>(&g, &first, &mut e),
                    _ => d_grad_u_constant::<3>(&g, &first, &mut e),
                }
                d_grad_u_general(std::hint::black_box(d), &g, &rep, &mut f);
                for (x, y) in e.iter().zip(&f) {
                    let ok = if payload {
                        x.to_bits() == y.to_bits()
                    } else {
                        same(*x, *y)
                    };
                    assert!(ok, "constant d={d}: {x:e} {y:e}");
                }
                for (x, y) in a.iter().zip(&b) {
                    let ok = if payload {
                        x.to_bits() == y.to_bits()
                    } else {
                        same(*x, *y)
                    };
                    assert!(ok, "d={d}: {x:e} {y:e}");
                }
            }
        }
    }

    /// When `temperature_term_vanishes` holds, `k + var·(T_q − T_ref)` is
    /// `k` bit for bit for every interpolated temperature the nodal values
    /// can produce (here: any combination of up to 27 nodal values with
    /// weights of magnitude below 16, the bound of the predicate's
    /// argument); and the predicate refuses the inputs for which it is not.
    #[test]
    fn vanishing_temperature_term_gives_the_conductivity_bitwise() {
        let mut v = Values::new(0x7e4a_0001);
        let mut checked = 0usize;
        for _ in 0..200_000 {
            let var = if v.next_u64() % 2 == 0 { 0.0 } else { -0.0 };
            let mut cond = [0.0; 4];
            v.fill(&mut cond);
            let mut nodal = [0.0; 27];
            v.fill(&mut nodal);
            // some draws at the bound or beyond
            if v.next_u64() % 4 == 0 {
                let k = (v.next_u64() % 27) as usize;
                nodal[k] = BOUNDED_TEMPERATURE * if v.next_u64() % 2 == 0 { 1.0 } else { -1.0 };
            }
            let t_ref = if v.next_u64() % 3 == 0 {
                -BOUNDED_TEMPERATURE
            } else {
                v.draw()
            };
            if !temperature_term_vanishes(var, t_ref, &cond, &nodal) {
                continue;
            }
            // an interpolation: weights below 16 in magnitude, sequential sum
            let mut tq = 0.0;
            for t in &nodal {
                let w = (v.next_u64() % 32) as Real / 2.0 - 8.0;
                tq += w * t;
            }
            assert!(tq.is_finite());
            let s = var * (tq - t_ref);
            for &k in &cond {
                assert_eq!((k + s).to_bits(), k.to_bits(), "k={k:e} s={s:e}");
            }
            checked += 1;
        }
        assert!(checked > 1_000, "{checked}");
        // refused: a non-zero variation, −0 or NaN conductivity entries,
        // non-finite or unbounded temperatures
        let ok_c = [1.0, 0.0, 0.0, 1.0];
        assert!(temperature_term_vanishes(0.0, 20.0, &ok_c, &[1.0, 2.0]));
        assert!(!temperature_term_vanishes(1e-300, 20.0, &ok_c, &[1.0]));
        assert!(!temperature_term_vanishes(Real::NAN, 20.0, &ok_c, &[1.0]));
        assert!(!temperature_term_vanishes(0.0, 20.0, &[1.0, -0.0], &[1.0]));
        assert!(!temperature_term_vanishes(0.0, 20.0, &[Real::NAN], &[1.0]));
        assert!(!temperature_term_vanishes(
            0.0,
            Real::INFINITY,
            &ok_c,
            &[1.0]
        ));
        assert!(!temperature_term_vanishes(0.0, 20.0, &ok_c, &[Real::NAN]));
        assert!(!temperature_term_vanishes(0.0, 20.0, &ok_c, &[1e300]));
    }
}
