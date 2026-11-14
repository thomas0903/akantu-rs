// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/boundary_conditions/{force_based_dirichlet.hh, spring_bc.hh,
// inclined_flat_dirichlet.hh}.

//! Boundary-condition helpers of the package (TASN-041…043).
//!
//! * [`ForceBasedDirichlet`]: the sub-boundaries move as a rigid body of mass
//!   `m` driven by a target force plus the integrated internal force
//!   (velocity-Verlet half steps), applied as a displacement increment.
//! * [`SpringBc`]: a [`ForceBasedDirichlet`] whose target force is that of a
//!   spring `−k · elongation`.
//! * [`InclinedFlatDirichlet`]: `u_axis = value + tang · (x_incl − center)`.
//!
//! **UD-403 (C, bit-rot):** upstream `InclinedFlatDirichlet::operator()` has
//! the pre-`VectorProxy` signature (`const`, `UInt node`, `Vector<…>`), so it
//! hides instead of overriding the virtual `DirichletFunctor::operator()`;
//! applied through `applyBC` it reaches the base `AKANTU_TO_IMPLEMENT()`.
//! The port implements the documented formula.

use std::collections::BTreeSet;

use akantu_core::enums::SpatialDirection;
use akantu_core::{Error, Idx, Real, Result};
use akantu_model::boundary_condition::DirichletFunctor;
use akantu_solid::SolidMechanicsModel;

use crate::boundary_functions::integrate_residual;

fn axis_index(axis: SpatialDirection) -> usize {
    match axis {
        SpatialDirection::X => 0,
        SpatialDirection::Y => 1,
        SpatialDirection::Z => 2,
    }
}

fn axis_slot(len: usize, axis: SpatialDirection) -> Result<usize> {
    let i = axis_index(axis);
    if i < len {
        Ok(i)
    } else {
        Err(Error::OutOfRange {
            what: "Dirichlet axis",
            index: i,
            len,
        })
    }
}

/// Upstream `ForceBasedDirichlet(model, axis, target_force, mass)` (a
/// `BC::Dirichlet::IncrementValue`).
#[derive(Clone, Debug)]
pub struct ForceBasedDirichlet {
    axis: SpatialDirection,
    /// Increment applied by the functor (upstream `value`).
    value: Real,
    subboundaries: BTreeSet<String>,
    mass: Real,
    velocity: Real,
    target_force: Real,
    total_residual: Real,
}

impl ForceBasedDirichlet {
    /// New condition (upstream default mass `0`, which makes the update
    /// divide by zero until [`Self::set_mass`] is called, as upstream).
    #[must_use]
    pub fn new(axis: SpatialDirection, target_force: Real, mass: Real) -> Self {
        Self {
            axis,
            value: 0.0,
            subboundaries: BTreeSet::new(),
            mass,
            velocity: 0.0,
            target_force,
            total_residual: 0.0,
        }
    }

    /// Upstream `insertSubBoundary(name)`.
    pub fn insert_sub_boundary(&mut self, name: &str) {
        self.subboundaries.insert(name.to_owned());
    }

    /// Upstream `setMass`.
    pub fn set_mass(&mut self, mass: Real) {
        self.mass = mass;
    }

    /// Upstream `setTargetForce`.
    pub fn set_target_force(&mut self, f: Real) {
        self.target_force = f;
    }

    /// Current increment (upstream `value`).
    #[must_use]
    pub fn value(&self) -> Real {
        self.value
    }

    /// Rigid-body velocity.
    #[must_use]
    pub fn velocity(&self) -> Real {
        self.velocity
    }

    /// Target force.
    #[must_use]
    pub fn target_force(&self) -> Real {
        self.target_force
    }

    /// Integrated internal force of the last update.
    #[must_use]
    pub fn total_residual(&self) -> Real {
        self.total_residual
    }

    /// Sub-boundaries.
    #[must_use]
    pub fn subboundaries(&self) -> &BTreeSet<String> {
        &self.subboundaries
    }

    /// Upstream `updateTotalResidual()`: the internal force along the axis
    /// summed over the nodes of every sub-boundary.
    ///
    /// # Errors
    /// Unknown group or internal force not allocated.
    pub fn update_total_residual(&mut self, model: &SolidMechanicsModel) -> Result<()> {
        self.total_residual = 0.0;
        for sb in &self.subboundaries {
            self.total_residual += integrate_residual(sb, model, axis_index(self.axis))?;
        }
        Ok(())
    }

    /// Upstream `update()`: half-step velocity update, the increment
    /// `v Δt + ½ Δt² a`, second half step; returns the integrated residual.
    ///
    /// # Errors
    /// See [`Self::update_total_residual`].
    pub fn update(&mut self, model: &SolidMechanicsModel) -> Result<Real> {
        self.update_total_residual(model)?;
        let total_force = self.target_force + self.total_residual;
        let a = total_force / self.mass;
        let dt = model.time_step(None)?;
        self.velocity += 0.5 * dt * a;
        self.value = self.velocity * dt + 0.5 * dt * dt * a;
        self.velocity += 0.5 * dt * a;
        Ok(self.total_residual)
    }

    /// Upstream `applyYourself()`: update, then apply the increment to every
    /// sub-boundary; returns the integrated residual.
    ///
    /// # Errors
    /// See [`Self::update`] and the boundary-condition application.
    pub fn apply_yourself(&mut self, model: &mut SolidMechanicsModel) -> Result<Real> {
        let reaction = self.update(model)?;
        self.apply_to_subboundaries(model)?;
        Ok(reaction)
    }

    fn apply_to_subboundaries(&self, model: &mut SolidMechanicsModel) -> Result<()> {
        for sb in &self.subboundaries {
            model.apply_dirichlet_bc(self, sb)?;
        }
        Ok(())
    }
}

impl DirichletFunctor for ForceBasedDirichlet {
    /// `BC::Dirichlet::IncrementValue`: block the axis and add the increment.
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        _coord: &[Real],
    ) -> Result<()> {
        let i = axis_slot(flags.len().min(primal.len()), self.axis)?;
        flags[i] = true;
        primal[i] += self.value;
        Ok(())
    }
}

/// Upstream `SpringBC(model, axis, stiffness, mass)`.
#[derive(Clone, Debug)]
pub struct SpringBc {
    base: ForceBasedDirichlet,
    stiffness: Real,
    elongation: Real,
}

impl SpringBc {
    /// New spring condition (target force `0`, elongation `0`).
    #[must_use]
    pub fn new(axis: SpatialDirection, stiffness: Real, mass: Real) -> Self {
        Self {
            base: ForceBasedDirichlet::new(axis, 0.0, mass),
            stiffness,
            elongation: 0.0,
        }
    }

    /// The underlying force-based condition.
    #[must_use]
    pub fn base(&self) -> &ForceBasedDirichlet {
        &self.base
    }

    /// Mutable underlying condition (sub-boundaries, mass).
    pub fn base_mut(&mut self) -> &mut ForceBasedDirichlet {
        &mut self.base
    }

    /// Upstream `getElongation`.
    #[must_use]
    pub fn elongation(&self) -> Real {
        self.elongation
    }

    /// Upstream `update()`: target force `−k · elongation`, base update,
    /// `elongation += increment`.
    ///
    /// # Errors
    /// See [`ForceBasedDirichlet::update`].
    pub fn update(&mut self, model: &SolidMechanicsModel) -> Result<Real> {
        self.base.target_force = -self.stiffness * self.elongation;
        let reaction = self.base.update(model)?;
        self.elongation += self.base.value;
        Ok(reaction)
    }

    /// Upstream `applyYourself()` (with the spring update).
    ///
    /// # Errors
    /// See [`Self::update`].
    pub fn apply_yourself(&mut self, model: &mut SolidMechanicsModel) -> Result<Real> {
        let reaction = self.update(model)?;
        self.base.apply_to_subboundaries(model)?;
        Ok(reaction)
    }

    /// Upstream `setToEquilibrium()`: target force balancing the current
    /// residual, and the matching elongation.
    ///
    /// # Errors
    /// See [`ForceBasedDirichlet::update_total_residual`].
    pub fn set_to_equilibrium(&mut self, model: &SolidMechanicsModel) -> Result<()> {
        self.base.update_total_residual(model)?;
        self.base.target_force = -self.base.total_residual;
        self.elongation = -self.base.target_force / self.stiffness;
        Ok(())
    }

    /// Upstream `incrementElongation(dx)` (`dx > 0` → negative target force).
    pub fn increment_elongation(&mut self, dx: Real) {
        self.elongation += dx;
    }
}

impl DirichletFunctor for SpringBc {
    fn apply(
        &self,
        node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        coord: &[Real],
    ) -> Result<()> {
        self.base.apply(node, flags, primal, coord)
    }
}

/// Upstream `InclinedFlatDirichlet(value, axis, incl_axis, center_coord,
/// tang)` (UD-403: implemented as documented).
#[derive(Clone, Copy, Debug)]
pub struct InclinedFlatDirichlet {
    /// Value at the center.
    pub value: Real,
    /// Blocked axis.
    pub axis: SpatialDirection,
    /// Axis along which the plane is inclined.
    pub incl_axis: SpatialDirection,
    /// Coordinate of the center along `incl_axis`.
    pub center_coord: Real,
    /// Slope.
    pub tang: Real,
}

impl DirichletFunctor for InclinedFlatDirichlet {
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        coord: &[Real],
    ) -> Result<()> {
        let j = axis_slot(coord.len(), self.incl_axis)?;
        let i = axis_slot(flags.len().min(primal.len()), self.axis)?;
        let dist = coord[j] - self.center_coord;
        flags[i] = true;
        primal[i] = self.value + self.tang * dist;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inclined_flat_dirichlet_formula() {
        let bc = InclinedFlatDirichlet {
            value: 0.5,
            axis: SpatialDirection::Y,
            incl_axis: SpatialDirection::X,
            center_coord: 1.0,
            tang: 0.25,
        };
        let mut flags = [false, false];
        let mut primal = [3.0, 7.0];
        bc.apply(0, &mut flags, &mut primal, &[3.0, -1.0]).unwrap();
        assert_eq!(flags, [false, true]);
        assert_eq!(primal, [3.0, 0.5 + 0.25 * 2.0]);
        let bad = InclinedFlatDirichlet {
            axis: SpatialDirection::Z,
            ..bc
        };
        assert!(bad.apply(0, &mut flags, &mut primal, &[0.0, 0.0]).is_err());
    }

    #[test]
    fn increment_functor() {
        let mut f = ForceBasedDirichlet::new(SpatialDirection::X, 1.0, 2.0);
        f.value = 0.125;
        let mut flags = [false; 2];
        let mut primal = [1.0, 1.0];
        f.apply(3, &mut flags, &mut primal, &[0.0, 0.0]).unwrap();
        assert_eq!(flags, [true, false]);
        assert_eq!(primal, [1.125, 1.0]);
    }
}
