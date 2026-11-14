// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS):
// src/model/structural_mechanics/structural_elements/
// structural_element_{bernoulli_beam_2,bernoulli_beam_3,kirchhoff_shell}.hh
// (computeTangentModuli), structural_mechanics_model_mass.cc
// (assembleLumpedMassMatrix, element part).

//! Element-level structural kernels used by `StructuralMechanicsModel`
//! that are pure element computations (STRUCT-003/004/005 tangent moduli,
//! STRUCT-010 element part of the lumped mass). Assembly, solvers and dumps
//! are in the structural model (`akantu-structural`).
//!
//! Tangent moduli are column-major `nb_stress²` matrices evaluated with the
//! upstream expressions (`E * A`, `D *= E t / (1 − ν²)`, `D · t³ / 12`).

use akantu_core::{Error, Real, Result};

/// Generalised tangent of `_bernoulli_beam_2` (`D = diag(E·A, E·I)`).
#[must_use]
pub fn tangent_bernoulli_beam_2(e: Real, a: Real, i: Real) -> [Real; 4] {
    [e * a, 0., 0., e * i]
}

/// Generalised tangent of `_bernoulli_beam_3`
/// (`D = diag(E·A, E·Iz, E·Iy, GJ)`).
#[must_use]
pub fn tangent_bernoulli_beam_3(e: Real, a: Real, iz: Real, iy: Real, gj: Real) -> [Real; 16] {
    let mut d = [0.0; 16];
    d[0] = e * a;
    d[5] = e * iz;
    d[10] = e * iy;
    d[15] = gj;
    d
}

/// Bending block of the DKT shell tangent (UD-007).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DktBendingStiffness {
    /// `E t³ / (12 (1 − ν²))` (plate rigidity; UD-007 decision). Evaluated
    /// as `D_m · (t·t) / 12` from the membrane block `D_m`, which equals the
    /// upstream value bit for bit when `t = 1`.
    #[default]
    Corrected,
    /// Upstream `D_m · t³ / 12 = E t⁴ / (12 (1 − ν²))` (extra factor `t`).
    Legacy,
}

/// Generalised tangent of `_discrete_kirchhoff_triangle_18` (6 × 6:
/// membrane block `D_m = E t/(1 − ν²) [[1, ν, 0], [ν, 1, 0], [0, 0,
/// (1 − ν)/2]]`, bending block per [`DktBendingStiffness`]).
#[must_use]
pub fn tangent_discrete_kirchhoff_triangle_18(
    e: Real,
    nu: Real,
    t: Real,
    bending: DktBendingStiffness,
) -> [Real; 36] {
    // Matrix<Real, 3, 3> D{{1., nu, 0.}, {nu, 1., 0.}, {0., 0., (1. - nu) / 2.}};
    let mut dm = [1., nu, 0., nu, 1., 0., 0., 0., (1. - nu) / 2.];
    // D *= m.E * m.t / (1. - m.nu * m.nu);
    let s = e * t / (1. - nu * nu);
    for v in &mut dm {
        *v *= s;
    }
    // Math::pow<3>(t) = (t * t) * t
    let factor = match bending {
        DktBendingStiffness::Legacy => t * t * t,
        DktBendingStiffness::Corrected => t * t,
    };
    let mut h = [0.0; 36];
    for i in 0..3 {
        for j in 0..3 {
            h[i + 6 * j] = dm[i + 3 * j];
            h[(i + 3) + 6 * (j + 3)] = dm[i + 3 * j] * factor / 12.;
        }
    }
    h
}

/// Element contributions to the lumped mass of a Bernoulli beam (element
/// part of `assembleLumpedMassMatrix`): returns `(m/2, V/2)` with
/// `L = |x₂ − x₁|` (dynamic vectors, sequential norm), `V = L·A`,
/// `m = V·ρ`. Upstream uses the first node twice (`node2 = conn(0)`), so
/// its masses are zero (UD-005); callers pass both nodes. The nodal
/// rotational inertia is not an element quantity; the structural model
/// computes it (UD-059).
///
/// # Errors
/// `Error::DimensionMismatch` unless `x1` and `x2` have the same length,
/// 2 (`_bernoulli_beam_2`) or 3 (`_bernoulli_beam_3`);
/// `Error::Numerical` for non-finite coordinates, density or area.
pub fn beam_lumped_mass_contributions(
    x1: &[Real],
    x2: &[Real],
    rho: Real,
    area: Real,
) -> Result<(Real, Real)> {
    let n = x1.len();
    if !(n == 2 || n == 3) {
        return Err(Error::DimensionMismatch {
            context: "beam lumped mass: coordinates of the first node (spatial dimension 2 or 3)",
            expected: if n < 2 { 2 } else { 3 },
            got: n,
        });
    }
    if x2.len() != n {
        return Err(Error::DimensionMismatch {
            context: "beam lumped mass: coordinates of the second node (same dimension)",
            expected: n,
            got: x2.len(),
        });
    }
    if let Some(v) = x1
        .iter()
        .chain(x2)
        .chain([&rho, &area])
        .find(|v| !v.is_finite())
    {
        return Err(Error::Numerical(format!(
            "beam lumped mass: non-finite input {v}"
        )));
    }
    let length = crate::eigen::norm(n, false, |i| x1[i] - x2[i]);
    let volume = length * area;
    let mass = volume * rho;
    Ok((mass / 2., volume / 2.))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dkt_tangent_blocks() {
        let h = tangent_discrete_kirchhoff_triangle_18(2.0, 0.25, 0.5, DktBendingStiffness::Legacy);
        let dm = 2.0 * 0.5 / (1. - 0.0625);
        assert_eq!(h[0], dm);
        assert_eq!(h[6], 0.25 * dm);
        assert_eq!(h[2 + 6 * 2], 0.375 * dm);
        assert_eq!(h[3 + 6 * 3], dm * 0.125 / 12.);
        let c =
            tangent_discrete_kirchhoff_triangle_18(2.0, 0.25, 0.5, DktBendingStiffness::Corrected);
        // plate rigidity E t³ / (12 (1 − ν²))
        let rigidity = 2.0 * 0.125 / (12. * (1. - 0.0625));
        assert!((c[3 + 6 * 3] - rigidity).abs() < 1e-16);
        assert_eq!(c[..18], h[..18]);
        // t = 1: both variants identical (bitwise)
        let a = tangent_discrete_kirchhoff_triangle_18(1.0, 0.3, 1.0, DktBendingStiffness::Legacy);
        let b =
            tangent_discrete_kirchhoff_triangle_18(1.0, 0.3, 1.0, DktBendingStiffness::Corrected);
        assert_eq!(a.map(Real::to_bits), b.map(Real::to_bits));
        // off-diagonal blocks are exactly zero
        assert!((0..3).all(|i| (3..6).all(|j| h[i + 6 * j] == 0. && h[j + 6 * i] == 0.)));
    }

    #[test]
    fn beam_tangents_and_lumped_contributions() {
        assert_eq!(
            tangent_bernoulli_beam_2(3e10, 0.01, 0.0025),
            [3e8, 0., 0., 7.5e7]
        );
        let d = tangent_bernoulli_beam_3(2., 3., 5., 7., 11.);
        assert_eq!((d[0], d[5], d[10], d[15]), (6., 10., 14., 11.));
        assert_eq!(d.iter().filter(|v| **v != 0.).count(), 4);
        let (m, v) =
            beam_lumped_mass_contributions(&[0., 0., 0.], &[3., 4., 12.], 2., 0.5).unwrap();
        assert_eq!((m, v), (6.5, 3.25));
    }
}
