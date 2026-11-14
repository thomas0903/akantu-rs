// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test support:
// a one-element model with a settable time step (upstream
// `SolidMechanicsModel::setTimeStep`, read by the viscoelastic laws through
// `getModel().getTimeStep()`), and helpers to drive one material point.
// Include with `mod common; #[path = "common/timed.rs"] mod timed;`.

#![allow(dead_code)] // each test binary uses a subset

use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::{
    InternalFieldMut, InternalFieldRef, LawContext, LawPhase, ModelServices,
};
use akantu_solid::Material;

use crate::common::{NG, SimplexModel};

/// [`SimplexModel`] with a time step.
pub struct TimedModel<'a> {
    pub inner: &'a SimplexModel,
    pub dt: Real,
}

impl ModelServices for TimedModel<'_> {
    fn spatial_dimension(&self) -> usize {
        self.inner.spatial_dimension()
    }
    fn mesh(&self) -> &Mesh {
        self.inner.mesh()
    }
    fn time_step(&self) -> Real {
        self.dt
    }
    fn nb_integration_points(&self, ty: ElementType, g: GhostType) -> usize {
        self.inner.nb_integration_points(ty, g)
    }
    fn integration_point_coordinates(
        &self,
        ty: ElementType,
        g: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()> {
        self.inner.integration_point_coordinates(ty, g, filter, out)
    }
    fn nodal_field(&self, name: &str) -> Result<akantu_core::array_shared::ArrayRef<'_, Real>> {
        self.inner.nodal_field(name)
    }
    fn gradient_on_integration_points(
        &self,
        field: &str,
        out: &mut Array<Real>,
        nb_component: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
    ) -> Result<()> {
        self.inner
            .gradient_on_integration_points(field, out, nb_component, ty, ghost, filter)
    }
    fn integrate_scalar(
        &self,
        f: &[Real],
        ty: ElementType,
        g: GhostType,
        filter: &[Idx],
    ) -> Result<Real> {
        self.inner.integrate_scalar(f, ty, g, filter)
    }
}

impl TimedModel<'_> {
    /// Phase context.
    pub fn ctx(&self, phase: LawPhase) -> LawContext<'_> {
        LawContext {
            phase,
            services: self,
        }
    }
}

/// The one-element model of the material probe (unit-ish simplex).
pub fn one_element(dim: usize) -> SimplexModel {
    let nodes: Vec<Vec<Real>> = match dim {
        1 => vec![vec![0.], vec![1.3]],
        2 => vec![vec![0., 0.], vec![1.1, 0.2], vec![0.3, 0.9]],
        _ => vec![
            vec![0., 0., 0.],
            vec![1.1, 0.1, 0.05],
            vec![0.2, 0.9, 0.1],
            vec![0.1, 0.2, 1.2],
        ],
    };
    let conn: Vec<Idx> = (0..=dim).map(|i| Idx::try_from(i).unwrap()).collect();
    SimplexModel::new(dim, &nodes, &[conn])
}

/// Column-major `dim × dim` from row-major rows.
pub fn col_major(rows: &[&[Real]]) -> Vec<Real> {
    let d = rows.len();
    let mut v = vec![0.0; d * d];
    for (r, row) in rows.iter().enumerate() {
        for (c, x) in row.iter().enumerate() {
            v[r + c * d] = *x;
        }
    }
    v
}

/// One quadrature point of a material: drives steps and reads results.
pub struct Point<'a> {
    pub law: &'a mut dyn Material,
    pub ty: ElementType,
}

impl Point<'_> {
    /// `grad_u ← g` (column-major) at every point of the block.
    pub fn set_grad_u(&mut self, g: &[Real]) {
        let mut a = self.law.base_mut().grad_u.get_mut(self.ty, NG).unwrap();
        for t in a.as_mut_slice().chunks_exact_mut(g.len()) {
            t.copy_from_slice(g);
        }
    }

    /// Stress of point 0 (column-major).
    pub fn stress(&self) -> Vec<Real> {
        self.law
            .base()
            .stress
            .get(self.ty, NG)
            .unwrap()
            .tuple(0)
            .to_vec()
    }

    /// Tuple 0 of a real internal.
    pub fn internal(&self, name: &str) -> Vec<Real> {
        let mut out = None;
        self.law.for_each_internal(&mut |n, f| {
            if n == name
                && let InternalFieldRef::Real(f) = f
            {
                out = Some(f.get(self.ty, NG).unwrap().tuple(0).to_vec());
            }
        });
        out.unwrap_or_else(|| panic!("internal {name}"))
    }

    /// Write a real internal at every point.
    pub fn write_internal(&mut self, name: &str, values: &[Real]) {
        self.law
            .for_each_internal_mut(&mut |n, f| {
                if n == name
                    && let InternalFieldMut::Real(f) = f
                {
                    let mut a = f.get_mut(self.ty, NG)?;
                    if a.nb_component() != values.len() {
                        return Err(Error::invalid("component count"));
                    }
                    for t in a.as_mut_slice().chunks_exact_mut(values.len()) {
                        t.copy_from_slice(values);
                    }
                }
                Ok(())
            })
            .unwrap();
    }

    /// One converged step: `beforeSolveStep`, `grad_u`, `computeStress`
    /// (`evaluations` times, as Newton iterations would), `afterSolveStep`.
    pub fn step(&mut self, model: &TimedModel<'_>, g: &[Real], evaluations: usize) {
        self.law
            .before_solve_step(&model.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
        self.set_grad_u(g);
        for _ in 0..evaluations {
            self.law
                .compute_stress(&model.ctx(LawPhase::Stress), self.ty, NG)
                .unwrap();
        }
        self.law
            .after_solve_step(&model.ctx(LawPhase::AfterSolveStep), true)
            .unwrap();
    }

    /// Tangent of point 0 (column-major Voigt).
    pub fn tangent(&mut self, model: &TimedModel<'_>) -> Vec<Real> {
        let dim = self.law.base().spatial_dimension;
        let v = dim * (dim + 1) / 2;
        let mut t = Array::zeros(1, v * v);
        self.law
            .compute_tangent_moduli(&model.ctx(LawPhase::Tangent), self.ty, NG, &mut t)
            .unwrap();
        t.as_slice().to_vec()
    }

    /// `getEnergy(id)`.
    pub fn energy(&mut self, model: &TimedModel<'_>, id: &str) -> Real {
        self.law
            .energy_of(&model.ctx(LawPhase::Energy), id)
            .unwrap()
    }
}
