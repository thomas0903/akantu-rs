// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Ported from Akantu (EPFL/LSMS):
// src/geometry/mesh_geom_common.hh (`cgal::Spherical =
// CGAL::Spherical_kernel_3<CGAL::Simple_cartesian<CGAL::Quotient<CGAL::MP_Float>>,
// CGAL::Algebraic_kernel_for_spheres_2_3<CGAL::Quotient<CGAL::MP_Float>>>`),
// aabb_primitives/line_arc.hh and the Circular_arc_point_3 overload of
// comparePoints in geom_helper_functions.hh.
//
// GPL-DERIVED: translation of CGAL 5.6.2 packages Circular_kernel_3 and
// Algebraic_kernel_for_spheres (GPL-3.0-or-later OR LicenseRef-Commercial;
// used here under GPL-3.0-or-later):
//   Circular_kernel_3/include/CGAL/Circular_kernel_3/Line_arc_3.h,
//   internal_functions_on_line_arc_3.h, internal_functions_on_sphere_3.h,
//   internal_function_has_on_spherical_kernel.h
//     Copyright (c) 2008 INRIA Sophia-Antipolis (France).
//     Author(s): Monique Teillaud, Sylvain Pion, Pedro Machado,
//     Julien Hazebrouck, Damien Leroy
//   Algebraic_kernel_for_spheres/include/CGAL/Root_for_spheres_2_3.h,
//   Polynomials_for_line_3.h,
//   Algebraic_kernel_for_spheres/internal_functions_on_roots_and_polynomial_1_3_and_2_3.h,
//   Algebraic_kernel_for_spheres/internal_functions_comparison_root_for_spheres_2_3.h
//     Copyright (c) 2005-2006 INRIA Sophia-Antipolis (France).
//     Author(s): Monique Teillaud
// The number types (MP_Float, Quotient, Sqrt_extension = Root_of_2) are the
// LGPL ports of akantu_geometry::number; the linear-kernel line
// construction (`Line_3(p, q) = (p, q − p)`) is CGAL's LGPL
// Cartesian_kernel.
// NOT GPL-derived: the sphere side predicates
// `SkSphere::{squared_distance_from_center, bounded_side,
// has_on_bounded_side, has_on_boundary, has_on_unbounded_side}` are the
// spherical kernel's inherited *linear-kernel* predicates, translated from
// CGAL 5.6.2 Cartesian_kernel (LGPL-3.0-or-later OR LicenseRef-Commercial):
//   Cartesian_kernel/include/CGAL/Cartesian/Sphere_3.h,
//   Cartesian_kernel/include/CGAL/constructions/kernel_ftC3.h
//   (`squared_distanceC3`)
//     Copyright (c) 2000 Utrecht University (The Netherlands), ETH Zurich
//     (Switzerland), INRIA Sophia-Antipolis (France), Max-Planck-Institute
//     Saarbruecken (Germany), and Tel-Aviv University (Israel).
//     Author(s): Andreas Fabri, Herve Bronnimann
// They live here only because `SkSphere` does; the LGPL code is conveyed
// under GPL-3.0-or-later as part of this crate.

//! The subset of upstream's exact spherical kernel used by the sphere
//! intersector (GEOM-008): points with `Quotient<MP_Float>` coordinates,
//! lines, spheres, circular-arc points (`Root_for_spheres_2_3`: coordinates
//! of type `Root_of_2`), line arcs, and `intersection(Line_arc_3,
//! Sphere_3)` with multiplicities.
//!
//! Every exact quantity is built with CGAL's operation sequence, so the
//! numerator/denominator representations — and therefore `to_double` of
//! the intersection points — are bit-identical to the oracle's. Predicates
//! (signs, comparisons) are exact.

use std::cmp::Ordering;

use akantu_core::math::are_float_equal;
use akantu_core::{Error, Real, Result};
use akantu_geometry::number::{Quotient, RootOf2};

/// `SK::Point_3`: Cartesian point with `Quotient<MP_Float>` coordinates.
#[derive(Clone, Debug)]
pub struct SkPoint {
    /// Coordinates.
    pub c: [Quotient; 3],
}

impl SkPoint {
    /// `SK::Point_3(x, y, z)` from doubles (`Quotient(double)` each).
    ///
    /// # Errors
    /// Non-finite coordinates.
    pub fn try_from_f64(p: [Real; 3]) -> Result<Self> {
        Ok(Self {
            c: [
                Quotient::try_from_f64(p[0])?,
                Quotient::try_from_f64(p[1])?,
                Quotient::try_from_f64(p[2])?,
            ],
        })
    }
}

/// `SK::Line_3`: a point and a direction vector (`Line_3(p, q) =
/// (p, Vector_3(p, q))`, `Vector_3(p, q) = q − p`).
#[derive(Clone, Debug)]
pub struct SkLine {
    /// `point()`.
    pub point: SkPoint,
    /// `to_vector()`.
    pub vector: [Quotient; 3],
}

impl SkLine {
    /// `Line_3(p, q)`.
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn through(p: &SkPoint, q: &SkPoint) -> Result<Self> {
        Ok(Self {
            point: p.clone(),
            vector: [
                q.c[0].sub(&p.c[0])?,
                q.c[1].sub(&p.c[1])?,
                q.c[2].sub(&p.c[2])?,
            ],
        })
    }

    /// `get_equation(Line_3)`: `Polynomials_for_line_3(a1, b1, a2, b2, a3,
    /// b3)` with `x = a1·t + b1`, … (`a = to_vector()`, `b = point()`).
    fn equation(&self) -> [(Quotient, Quotient); 3] {
        [
            (self.vector[0].clone(), self.point.c[0].clone()),
            (self.vector[1].clone(), self.point.c[1].clone()),
            (self.vector[2].clone(), self.point.c[2].clone()),
        ]
    }
}

/// `SK::Sphere_3(center, squared_radius)`.
#[derive(Clone, Debug)]
pub struct SkSphere {
    /// `center()`.
    pub center: SkPoint,
    /// `squared_radius()`.
    pub squared_radius: Quotient,
}

impl SkSphere {
    /// `Sphere_3(Point_3(x, y, z), r²)` from doubles.
    ///
    /// # Errors
    /// Non-finite input.
    pub fn try_from_f64(center: [Real; 3], squared_radius: Real) -> Result<Self> {
        Ok(Self {
            center: SkPoint::try_from_f64(center)?,
            squared_radius: Quotient::try_from_f64(squared_radius)?,
        })
    }

    /// `squared_distance(center(), p)`: CGAL's `squared_distanceC3(cx, cy,
    /// cz, px, py, pz) = square(cx − px) + square(cy − py) + square(cz − pz)`
    /// in exact `Quotient<MP_Float>` arithmetic (LGPL Cartesian_kernel; not
    /// GPL-derived, see the file header).
    ///
    /// # Errors
    /// Exponent overflow of the exact arithmetic.
    pub fn squared_distance_from_center(&self, p: &SkPoint) -> Result<Quotient> {
        let c = &self.center.c;
        let dx = c[0].sub(&p.c[0])?.square()?;
        let dy = c[1].sub(&p.c[1])?.square()?;
        let dz = c[2].sub(&p.c[2])?.square()?;
        dx.add(&dy)?.add(&dz)
    }

    /// `Sphere_3::bounded_side(p)`:
    /// `enum_cast<Bounded_side>(compare(squared_radius(),
    /// squared_distance(center(), p)))`, exact (LGPL Cartesian_kernel
    /// `SphereC3`; not GPL-derived).
    ///
    /// # Errors
    /// Exponent overflow of the exact arithmetic.
    pub fn bounded_side(&self, p: &SkPoint) -> Result<BoundedSide> {
        let d2 = self.squared_distance_from_center(p)?;
        Ok(match self.squared_radius.compare(&d2)? {
            Ordering::Less => BoundedSide::OnUnboundedSide,
            Ordering::Equal => BoundedSide::OnBoundary,
            Ordering::Greater => BoundedSide::OnBoundedSide,
        })
    }

    /// `Sphere_3::has_on_bounded_side(p)`: `squared_distance(center(), p) <
    /// squared_radius()`, exact.
    ///
    /// # Errors
    /// Exponent overflow of the exact arithmetic.
    pub fn has_on_bounded_side(&self, p: &SkPoint) -> Result<bool> {
        Ok(self
            .squared_distance_from_center(p)?
            .compare(&self.squared_radius)?
            == Ordering::Less)
    }

    /// `Sphere_3::has_on_boundary(p)`: `squared_distance(center(), p) ==
    /// squared_radius()`, exact.
    ///
    /// # Errors
    /// Exponent overflow of the exact arithmetic.
    pub fn has_on_boundary(&self, p: &SkPoint) -> Result<bool> {
        self.squared_distance_from_center(p)?
            .eq_value(&self.squared_radius)
    }

    /// `Sphere_3::has_on_unbounded_side(p)`: is `squared_distance(center(),
    /// p)` greater than `squared_radius()`? Exact. IGFEM's
    /// `IGFEMEnrichment::isInside` (`igfem_enrichment_inline_impl.hh:37`)
    /// tests `not sphere.has_on_unbounded_side(SK::Point_3(x, y, 0))`.
    ///
    /// # Errors
    /// Exponent overflow of the exact arithmetic.
    pub fn has_on_unbounded_side(&self, p: &SkPoint) -> Result<bool> {
        Ok(self
            .squared_distance_from_center(p)?
            .compare(&self.squared_radius)?
            == Ordering::Greater)
    }
}

/// CGAL `Bounded_side` (`ON_UNBOUNDED_SIDE = -1`, `ON_BOUNDARY = 0`,
/// `ON_BOUNDED_SIDE = 1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundedSide {
    /// Outside the sphere.
    OnUnboundedSide,
    /// On the sphere.
    OnBoundary,
    /// Inside the sphere.
    OnBoundedSide,
}

/// `SK::Circular_arc_point_3` (`Root_for_spheres_2_3`: three `Root_of_2`
/// coordinates).
#[derive(Clone, Debug)]
pub struct CircularArcPoint {
    /// Coordinates.
    pub c: [RootOf2; 3],
}

impl CircularArcPoint {
    /// `Circular_arc_point_3(Point_3)`: rational coordinates.
    #[must_use]
    pub fn from_point(p: &SkPoint) -> Self {
        Self {
            c: [
                RootOf2::from_rational(p.c[0].clone()),
                RootOf2::from_rational(p.c[1].clone()),
                RootOf2::from_rational(p.c[2].clone()),
            ],
        }
    }

    /// `(to_double(x()), to_double(y()), to_double(z()))`.
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn to_double(&self) -> Result<[Real; 3]> {
        Ok([
            self.c[0].to_double()?,
            self.c[1].to_double()?,
            self.c[2].to_double()?,
        ])
    }

    /// `compare_xyz` (`AlgebraicSphereFunctors::compare_xyz`, exact).
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn compare_xyz(&self, o: &Self) -> Result<Ordering> {
        for k in 0..3 {
            let c = self.c[k].compare(&o.c[k])?;
            if c != Ordering::Equal {
                return Ok(c);
            }
        }
        Ok(Ordering::Equal)
    }
}

/// Akantu `comparePoints(Circular_arc_point_3, Circular_arc_point_3)`:
/// `Math::are_float_equal` on the `to_double` coordinates.
///
/// # Errors
/// Exponent overflow.
pub fn compare_arc_points(
    a: &CircularArcPoint,
    b: &CircularArcPoint,
    tolerance: Real,
) -> Result<bool> {
    let (x, y) = (a.to_double()?, b.to_double()?);
    Ok((0..3).all(|k| are_float_equal(x[k], y[k], tolerance)))
}

/// `SK::Line_arc_3(line, source, target)`.
#[derive(Clone, Debug)]
pub struct LineArc {
    line: SkLine,
    source: CircularArcPoint,
    target: CircularArcPoint,
}

impl LineArc {
    /// `Line_arc_3(Line_3(a, b), a, b)` from two points (Akantu's
    /// construction of a triangle edge).
    ///
    /// # Errors
    /// Coincident points (upstream would divide by zero in the solver; see
    /// UD-409), exponent overflow.
    pub fn from_points(a: &SkPoint, b: &SkPoint) -> Result<Self> {
        let line = SkLine::through(a, b)?;
        if line.vector.iter().all(Quotient::is_zero) {
            return Err(Error::InvalidArgument(
                "Line_arc_3: coincident end points (degenerate mesh edge)".into(),
            ));
        }
        Ok(Self {
            line,
            source: CircularArcPoint::from_point(a),
            target: CircularArcPoint::from_point(b),
        })
    }

    /// `source()`.
    #[must_use]
    pub fn source(&self) -> &CircularArcPoint {
        &self.source
    }

    /// `target()`.
    #[must_use]
    pub fn target(&self) -> &CircularArcPoint {
        &self.target
    }

    /// `supporting_line()`.
    #[must_use]
    pub fn supporting_line(&self) -> &SkLine {
        &self.line
    }

    /// `has_on(line_arc, p, has_on_supporting_line = true)`:
    /// `lower_xyz_extremity ≤ p ≤ higher_xyz_extremity` (xyz order).
    fn has_on(&self, p: &CircularArcPoint) -> Result<bool> {
        let begin_less = self.source.compare_xyz(&self.target)? == Ordering::Less;
        let (lo, hi) = if begin_less {
            (&self.source, &self.target)
        } else {
            (&self.target, &self.source)
        };
        Ok(lo.compare_xyz(p)? != Ordering::Greater && p.compare_xyz(hi)? != Ordering::Greater)
    }

    /// `CGAL::intersection(Line_arc_3, Sphere_3)`: the solutions of the
    /// supporting line that lie on the arc, with their multiplicity
    /// (`2` for a tangent point).
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn intersect_sphere(&self, s: &SkSphere) -> Result<Vec<(CircularArcPoint, u32)>> {
        let solutions = solve_line_sphere(&self.line, s)?;
        let mut out = Vec::with_capacity(solutions.len());
        for (p, m) in solutions {
            if self.has_on(&p)? {
                out.push((p, m));
            }
        }
        Ok(out)
    }
}

/// `AlgebraicSphereFunctors::solve(Polynomials_for_line_3,
/// Polynomial_for_spheres_2_3)` (via `intersect_3(Sphere_3, Line_3)`).
///
/// # Errors
/// Exponent overflow; a degenerate line (CGAL precondition; impossible for
/// [`LineArc`]).
pub fn solve_line_sphere(line: &SkLine, s: &SkSphere) -> Result<Vec<(CircularArcPoint, u32)>> {
    let [(a1, b1), (a2, b2), (a3, b3)] = line.equation();
    if a1.is_zero() && a2.is_zero() && a3.is_zero() {
        return Err(Error::InvalidArgument("solve: degenerate line".into()));
    }
    let (sa, sb, sc) = (&s.center.c[0], &s.center.c[1], &s.center.c[2]);
    let r_sq = &s.squared_radius;
    let sq_a1 = a1.square()?;
    let sq_a2 = a2.square()?;
    let sq_a3 = a3.square()?;
    let sq_a = sa.square()?;
    let sq_b = sb.square()?;
    let sq_c = sc.square()?;
    // a = sq_a1 + sq_a2 + sq_a3
    let a = sq_a1.add(&sq_a2)?.add(&sq_a3)?;
    // b = a1·(b1 − sa) + a2·(b2 − sb) + a3·(b3 − sc)
    let b = a1
        .mul(&b1.sub(sa)?)?
        .add(&a2.mul(&b2.sub(sb)?)?)?
        .add(&a3.mul(&b3.sub(sc)?)?)?;
    // c = b1² + b2² + b3² + sq_a + sq_b + sq_c − 2·(sa·b1 + sb·b2 + sc·b3) − r²
    let c = b1
        .square()?
        .add(&b2.square()?)?
        .add(&b3.square()?)?
        .add(&sq_a)?
        .add(&sq_b)?
        .add(&sq_c)?
        .sub(
            &sa.mul(&b1)?
                .add(&sb.mul(&b2)?)?
                .add(&sc.mul(&b3)?)?
                .mul_i32(2)?,
        )?
        .sub(r_sq)?;
    let alpha = b.neg().div(&a)?;
    let gama = alpha.square()?.sub(&c.div(&a)?)?;

    if gama.sign() == Ordering::Less {
        return Ok(Vec::new());
    }
    if gama.is_zero() {
        // Tangent point: Root_of_2(a_i·alpha + b_i), multiplicity 2.
        let coord = |ai: &Quotient, bi: &Quotient| -> Result<RootOf2> {
            Ok(RootOf2::from_rational(ai.mul(&alpha)?.add(bi)?))
        };
        let p = CircularArcPoint {
            c: [coord(&a1, &b1)?, coord(&a2, &b2)?, coord(&a3, &b3)?],
        };
        return Ok(vec![(p, 2)]);
    }
    let t1 = RootOf2::new(alpha.clone(), Quotient::from_i32(-1), gama.clone());
    let t2 = RootOf2::new(alpha, Quotient::from_i32(1), gama);
    let first_t1 = match (a1.sign(), a2.sign()) {
        (Ordering::Equal, Ordering::Equal) => a3.sign() == Ordering::Greater,
        (Ordering::Equal, s2) => s2 == Ordering::Greater,
        (s1, _) => s1 == Ordering::Greater,
    };
    // p.a_i · t + p.b_i (Boost: copy t, `*= a_i`, then `+= b_i`).
    let point = |t: &RootOf2| -> Result<CircularArcPoint> {
        Ok(CircularArcPoint {
            c: [
                t.mul_rational(&a1)?.add_rational(&b1)?,
                t.mul_rational(&a2)?.add_rational(&b2)?,
                t.mul_rational(&a3)?.add_rational(&b3)?,
            ],
        })
    };
    let (p1, p2) = (point(&t1)?, point(&t2)?);
    Ok(if first_t1 {
        vec![(p1, 1), (p2, 1)]
    } else {
        vec![(p2, 1), (p1, 1)]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64, z: f64) -> SkPoint {
        SkPoint::try_from_f64([x, y, z]).unwrap()
    }

    #[test]
    fn crossing_tangent_and_missing_arcs() {
        let s = SkSphere::try_from_f64([0.0, 0.0, 0.0], 1.0).unwrap();
        // Crossing the unit circle once at (1, 0).
        let arc = LineArc::from_points(&pt(0.0, 0.0, 0.0), &pt(2.0, 0.0, 0.0)).unwrap();
        let r = arc.intersect_sphere(&s).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].1, 1);
        assert_eq!(r[0].0.to_double().unwrap(), [1.0, 0.0, 0.0]);
        // Two crossings.
        let arc2 = LineArc::from_points(&pt(-2.0, 0.0, 0.0), &pt(2.0, 0.0, 0.0)).unwrap();
        assert_eq!(arc2.intersect_sphere(&s).unwrap().len(), 2);
        // Tangent at (0, 1).
        let arc3 = LineArc::from_points(&pt(-1.0, 1.0, 0.0), &pt(1.0, 1.0, 0.0)).unwrap();
        let r3 = arc3.intersect_sphere(&s).unwrap();
        assert_eq!(r3.len(), 1);
        assert_eq!(r3[0].1, 2);
        assert_eq!(r3[0].0.to_double().unwrap(), [0.0, 1.0, 0.0]);
        // Miss.
        let arc4 = LineArc::from_points(&pt(-1.0, 2.0, 0.0), &pt(1.0, 2.0, 0.0)).unwrap();
        assert!(arc4.intersect_sphere(&s).unwrap().is_empty());
        // Degenerate edge.
        assert!(LineArc::from_points(&pt(1.0, 2.0, 0.0), &pt(1.0, 2.0, 0.0)).is_err());
    }
}
