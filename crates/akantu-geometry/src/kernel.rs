// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_geom_common.hh
// (`cgal::Cartesian = CGAL::Simple_cartesian<Real>`).
// Derived from CGAL 5.6.2, LGPL-3.0-or-later (CGAL's Kernel_23 and
// Cartesian_kernel packages):
//   Cartesian_kernel/include/CGAL/Cartesian/function_objects.h,
//   Cartesian/{Segment_3,Ray_3,Line_3,Vector_3,Tetrahedron_3,solve_3}.h,
//   Cartesian_kernel/include/CGAL/predicates/kernel_ftC2.h, kernel_ftC3.h,
//   Cartesian_kernel/include/CGAL/constructions/kernel_ftC3.h,
//   Kernel_23/include/CGAL/{determinant.h,Kernel/solve.h,Bbox_3.h},
//   Kernel_23/include/CGAL/predicates/sign_of_determinant.h
//     Copyright (c) 1999-2016 Utrecht University (The Netherlands),
//     ETH Zurich (Switzerland), INRIA Sophia-Antipolis (France),
//     Max-Planck-Institute Saarbruecken (Germany), and Tel-Aviv University
//     (Israel). Author(s): Andreas Fabri, Herve Bronnimann, Sylvain Pion,
//     Stefan Schirra, Michael Hoffmann, Olivier Devillers, Mariette Yvinec
// No GPL-licensed CGAL code is used in this file (D-042 (3), D-022).

//! The inexact `CGAL::Simple_cartesian<double>` kernel used by upstream's
//! segment intersector (`cgal::Cartesian`): objects, predicates and
//! constructions with CGAL's formulas and operation order, so that results
//! are bit-for-bit those of the oracle. Predicates are evaluated in `f64`
//! exactly as CGAL does (they are *not* exact; [`crate::intersection`] has
//! exact `orient2d`/`orient3d` for other uses).
//!
//! The oracle is compiled with `-DNDEBUG`, so CGAL's precondition and
//! assertion macros are inactive: the port follows the same code paths on
//! degenerate inputs and reports an error only where CGAL calls
//! `CGAL_error()` (always active, throws).

use std::cmp::Ordering;

use akantu_core::Real;

/// CGAL `Sign` / `Orientation` / `Comparison_result`
/// (`COLLINEAR = COPLANAR = ZERO`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sign {
    /// `NEGATIVE` (`CLOCKWISE`, `SMALLER`).
    Negative,
    /// `ZERO` (`COLLINEAR`, `COPLANAR`, `EQUAL`).
    Zero,
    /// `POSITIVE` (`COUNTERCLOCKWISE`, `LARGER`).
    Positive,
}

impl Sign {
    /// `enum_cast<Sign>(compare(a, b))` for doubles
    /// (`a < b ? SMALLER : (b < a ? LARGER : EQUAL)`).
    #[inline]
    #[must_use]
    pub fn compare(a: Real, b: Real) -> Self {
        if a < b {
            Self::Negative
        } else if b < a {
            Self::Positive
        } else {
            Self::Zero
        }
    }

    /// `CGAL::sign(double)`.
    #[inline]
    #[must_use]
    pub fn of(x: Real) -> Self {
        Self::compare(x, 0.0)
    }

    /// Product of signs (`Orientation * Orientation`).
    #[inline]
    #[must_use]
    pub fn times(self, o: Self) -> Self {
        match (self, o) {
            (Self::Zero, _) | (_, Self::Zero) => Self::Zero,
            (a, b) if a == b => Self::Positive,
            _ => Self::Negative,
        }
    }

    /// As a `std::cmp::Ordering`.
    #[must_use]
    pub fn to_ordering(self) -> Ordering {
        match self {
            Self::Negative => Ordering::Less,
            Self::Zero => Ordering::Equal,
            Self::Positive => Ordering::Greater,
        }
    }
}

/// `Point_3` of `Simple_cartesian<double>`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point3 {
    /// x.
    pub x: Real,
    /// y.
    pub y: Real,
    /// z.
    pub z: Real,
}

/// `Vector_3` of `Simple_cartesian<double>`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vector3 {
    /// x.
    pub x: Real,
    /// y.
    pub y: Real,
    /// z.
    pub z: Real,
}

impl Point3 {
    /// Point from coordinates.
    #[inline]
    #[must_use]
    pub const fn new(x: Real, y: Real, z: Real) -> Self {
        Self { x, y, z }
    }

    /// Coordinates as an array.
    #[inline]
    #[must_use]
    pub const fn to_array(self) -> [Real; 3] {
        [self.x, self.y, self.z]
    }

    /// Coordinate `i` (`p[i]`, `0..3`).
    #[inline]
    #[must_use]
    pub fn coord(self, i: usize) -> Real {
        match i {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }

    /// CGAL homogeneous constructor `Point_3(x, y, z, w)` of the Cartesian
    /// kernel: `w != 1 ? (x/w, y/w, z/w) : (x, y, z)`.
    #[inline]
    #[must_use]
    pub fn from_homogeneous(x: Real, y: Real, z: Real, w: Real) -> Self {
        if w == 1.0 {
            Self { x, y, z }
        } else {
            Self {
                x: x / w,
                y: y / w,
                z: z / w,
            }
        }
    }

    /// `p + v` (`Construct_translated_point_3`).
    #[inline]
    #[must_use]
    pub fn translate(self, v: Vector3) -> Self {
        Self {
            x: self.x + v.x,
            y: self.y + v.y,
            z: self.z + v.z,
        }
    }

    /// Degenerate bounding box (`Construct_bbox_3(Point_3)`).
    #[inline]
    #[must_use]
    pub fn bbox(self) -> Bbox3 {
        Bbox3 {
            min: self.to_array(),
            max: self.to_array(),
        }
    }
}

impl Vector3 {
    /// Vector from coordinates.
    #[inline]
    #[must_use]
    pub const fn new(x: Real, y: Real, z: Real) -> Self {
        Self { x, y, z }
    }

    /// `Vector_3(p, q) = q − p` (`Construct_vector_3`).
    #[inline]
    #[must_use]
    pub fn from_points(p: Point3, q: Point3) -> Self {
        Self {
            x: q.x - p.x,
            y: q.y - p.y,
            z: q.z - p.z,
        }
    }

    /// `Construct_cross_product_vector_3`.
    #[inline]
    #[must_use]
    pub fn cross(self, w: Self) -> Self {
        Self {
            x: self.y * w.z - self.z * w.y,
            y: self.z * w.x - self.x * w.z,
            z: self.x * w.y - self.y * w.x,
        }
    }

    /// `Compute_scalar_product_3`: `v.x·w.x + v.y·w.y + v.z·w.z`.
    #[inline]
    #[must_use]
    pub fn dot(self, w: Self) -> Real {
        self.x * w.x + self.y * w.y + self.z * w.z
    }

    /// `squared_length`: `x² + y² + z²`.
    #[inline]
    #[must_use]
    pub fn squared_length(self) -> Real {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// `c · w` (`Construct_scaled_vector_3`: `(c·w.x, c·w.y, c·w.z)`).
    #[inline]
    #[must_use]
    pub fn scale(self, c: Real) -> Self {
        Self {
            x: c * self.x,
            y: c * self.y,
            z: c * self.z,
        }
    }
}

/// `Bbox_3`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bbox3 {
    /// `(xmin, ymin, zmin)`.
    pub min: [Real; 3],
    /// `(xmax, ymax, zmax)`.
    pub max: [Real; 3],
}

/// `std::min` (returns the first argument when equivalent).
#[inline]
fn std_min(a: Real, b: Real) -> Real {
    if b < a { b } else { a }
}

/// `std::max` (returns the first argument when equivalent).
#[inline]
fn std_max(a: Real, b: Real) -> Real {
    if a < b { b } else { a }
}

impl Bbox3 {
    /// `Bbox_3::operator+`.
    #[inline]
    #[must_use]
    pub fn union(&self, b: &Self) -> Self {
        Self {
            min: [
                std_min(self.min[0], b.min[0]),
                std_min(self.min[1], b.min[1]),
                std_min(self.min[2], b.min[2]),
            ],
            max: [
                std_max(self.max[0], b.max[0]),
                std_max(self.max[1], b.max[1]),
                std_max(self.max[2], b.max[2]),
            ],
        }
    }
}

/// `Segment_3`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment3 {
    /// `source()`.
    pub source: Point3,
    /// `target()`.
    pub target: Point3,
}

impl Segment3 {
    /// Segment from its end points.
    #[inline]
    #[must_use]
    pub const fn new(source: Point3, target: Point3) -> Self {
        Self { source, target }
    }

    /// `is_degenerate()`: `source == target`.
    #[inline]
    #[must_use]
    pub fn is_degenerate(&self) -> bool {
        self.source == self.target
    }

    /// `min()`: the lexicographically (xyz) smaller end point
    /// (`lexicographically_xyz_smaller(source, target) ? source : target`).
    #[inline]
    #[must_use]
    pub fn min(&self) -> Point3 {
        if compare_xyz(self.source, self.target) == Sign::Negative {
            self.source
        } else {
            self.target
        }
    }

    /// `max()`.
    #[inline]
    #[must_use]
    pub fn max(&self) -> Point3 {
        if compare_xyz(self.source, self.target) == Sign::Negative {
            self.target
        } else {
            self.source
        }
    }

    /// `supporting_line()`: `Line_3(source, Vector_3(source, target))`.
    #[inline]
    #[must_use]
    pub fn supporting_line(&self) -> Line3 {
        Line3 {
            point: self.source,
            vector: Vector3::from_points(self.source, self.target),
        }
    }

    /// `point(i)` for `i ∈ {0, 1}` (`vertex(i)`).
    #[inline]
    #[must_use]
    pub fn point(&self, i: usize) -> Point3 {
        if i.is_multiple_of(2) {
            self.source
        } else {
            self.target
        }
    }

    /// Bounding box (`source.bbox() + target.bbox()`).
    #[inline]
    #[must_use]
    pub fn bbox(&self) -> Bbox3 {
        self.source.bbox().union(&self.target.bbox())
    }
}

/// `Ray_3` (stored as source and second point).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray3 {
    /// `source()`.
    pub source: Point3,
    /// `second_point()`.
    pub second: Point3,
}

impl Ray3 {
    /// `Ray_3(p, q)`.
    #[inline]
    #[must_use]
    pub const fn new(source: Point3, second: Point3) -> Self {
        Self { source, second }
    }

    /// `Ray_3(p, v)`: `(p, p + v)`.
    #[inline]
    #[must_use]
    pub fn from_vector(p: Point3, v: Vector3) -> Self {
        Self {
            source: p,
            second: p.translate(v),
        }
    }

    /// `to_vector()`: `second_point − source`.
    #[inline]
    #[must_use]
    pub fn to_vector(&self) -> Vector3 {
        Vector3::from_points(self.source, self.second)
    }

    /// `is_degenerate()`: `source == second_point`.
    #[inline]
    #[must_use]
    pub fn is_degenerate(&self) -> bool {
        self.source == self.second
    }
}

/// `Line_3` (stored as a point and a vector).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line3 {
    /// `point()`.
    pub point: Point3,
    /// `to_vector()` (also `direction()`'s `dx, dy, dz`).
    pub vector: Vector3,
}

/// `Plane_3` (`a x + b y + c z + d = 0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane3 {
    /// a.
    pub a: Real,
    /// b.
    pub b: Real,
    /// c.
    pub c: Real,
    /// d.
    pub d: Real,
}

impl Plane3 {
    /// `plane_from_pointsC3(p, q, r)`.
    #[must_use]
    pub fn from_points(p: Point3, q: Point3, r: Point3) -> Self {
        let rpx = p.x - r.x;
        let rpy = p.y - r.y;
        let rpz = p.z - r.z;
        let rqx = q.x - r.x;
        let rqy = q.y - r.y;
        let rqz = q.z - r.z;
        let a = rpy * rqz - rqy * rpz;
        let b = rpz * rqx - rqz * rpx;
        let c = rpx * rqy - rqx * rpy;
        let d = -a * r.x - b * r.y - c * r.z;
        Self { a, b, c, d }
    }
}

/// `Triangle_3`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle3 {
    /// Vertices.
    pub v: [Point3; 3],
}

impl Triangle3 {
    /// Triangle from three points.
    #[inline]
    #[must_use]
    pub const fn new(a: Point3, b: Point3, c: Point3) -> Self {
        Self { v: [a, b, c] }
    }

    /// `vertex(i)`.
    #[inline]
    #[must_use]
    pub fn vertex(&self, i: usize) -> Point3 {
        self.v[i % 3]
    }

    /// `supporting_plane()`: `plane_from_points(v0, v1, v2)`.
    #[inline]
    #[must_use]
    pub fn supporting_plane(&self) -> Plane3 {
        Plane3::from_points(self.v[0], self.v[1], self.v[2])
    }

    /// `Construct_bbox_3(Triangle_3)`: `bbox(v0) + bbox(v1) + bbox(v2)`.
    #[inline]
    #[must_use]
    pub fn bbox(&self) -> Bbox3 {
        self.v[0]
            .bbox()
            .union(&self.v[1].bbox())
            .union(&self.v[2].bbox())
    }

    /// `is_degenerate()`: `collinear(v0, v1, v2)`.
    #[inline]
    #[must_use]
    pub fn is_degenerate(&self) -> bool {
        collinear(self.v[0], self.v[1], self.v[2])
    }
}

/// CGAL `Bounded_side`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundedSide {
    /// `ON_UNBOUNDED_SIDE`.
    Unbounded,
    /// `ON_BOUNDARY`.
    Boundary,
    /// `ON_BOUNDED_SIDE`.
    Bounded,
}

/// `Tetrahedron_3`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tetrahedron3 {
    /// Vertices.
    pub v: [Point3; 4],
}

impl Tetrahedron3 {
    /// Tetrahedron from four points.
    #[inline]
    #[must_use]
    pub const fn new(a: Point3, b: Point3, c: Point3, d: Point3) -> Self {
        Self { v: [a, b, c, d] }
    }

    /// `orientation()`.
    #[must_use]
    pub fn orientation(&self) -> Sign {
        orientation(self.v[0], self.v[1], self.v[2], self.v[3])
    }

    /// `bounded_side(p)` (Cartesian `Bounded_side_3` via `solve`).
    #[must_use]
    pub fn bounded_side(&self, p: Point3) -> BoundedSide {
        let v0 = Vector3::from_points(self.v[0], self.v[1]);
        let v1 = Vector3::from_points(self.v[0], self.v[2]);
        let v2 = Vector3::from_points(self.v[0], self.v[3]);
        let d = Vector3::from_points(self.v[0], p);
        let (alpha, beta, gamma, denom) = solve_with_denom(v0, v1, v2, d);
        if alpha < 0.0 || beta < 0.0 || gamma < 0.0 || alpha + beta + gamma > denom {
            return BoundedSide::Unbounded;
        }
        if alpha == 0.0 || beta == 0.0 || gamma == 0.0 || alpha + beta + gamma == denom {
            return BoundedSide::Boundary;
        }
        BoundedSide::Bounded
    }

    /// `has_on_bounded_side(p)`.
    #[must_use]
    pub fn has_on_bounded_side(&self, p: Point3) -> bool {
        self.bounded_side(p) == BoundedSide::Bounded
    }

    /// `has_on_boundary(p)`: `oriented_side(p) == ON_ORIENTED_BOUNDARY`,
    /// i.e. `bounded_side(p) · orientation() == 0` (a degenerate
    /// tetrahedron reports every point on its boundary).
    #[must_use]
    pub fn has_on_boundary(&self, p: Point3) -> bool {
        let o = self.orientation();
        if o == Sign::Zero {
            return true;
        }
        self.bounded_side(p) == BoundedSide::Boundary
    }
}

/// CGAL `solve(a, b, c, d, x, y, z, denom)` (`Kernel/solve.h`): Cramer's
/// rule with a positive denominator.
fn solve_with_denom(a: Vector3, b: Vector3, c: Vector3, d: Vector3) -> (Real, Real, Real, Real) {
    let (a1, a2, a3) = (a.x, a.y, a.z);
    let (b1, b2, b3) = (b.x, b.y, b.z);
    let (c1, c2, c3) = (c.x, c.y, c.z);
    let (d1, d2, d3) = (d.x, d.y, d.z);
    let ab23 = a3 * b2 - a2 * b3;
    let ab13 = a3 * b1 - a1 * b3;
    let ab12 = a2 * b1 - a1 * b2;
    let mut denom = ab23 * c1 - ab13 * c2 + ab12 * c3;
    let cd23 = c3 * d2 - c2 * d3;
    let cd13 = c3 * d1 - c1 * d3;
    let cd12 = c2 * d1 - c1 * d2;
    let mut x = b3 * cd12 - b2 * cd13 + b1 * cd23;
    let mut y = a2 * cd13 - cd12 * a3 - cd23 * a1;
    let mut z = ab23 * d1 + ab12 * d3 - ab13 * d2;
    if denom < 0.0 {
        denom = -denom;
        x = -x;
        y = -y;
        z = -z;
    }
    (x, y, z, denom)
}

/// CGAL `sign_of_determinant(a00, a01, a10, a11)` =
/// `compare(a00·a11, a10·a01)`.
#[inline]
#[must_use]
pub fn sign_of_determinant2(a00: Real, a01: Real, a10: Real, a11: Real) -> Sign {
    Sign::compare(a00 * a11, a10 * a01)
}

/// CGAL `determinant` (3×3, row-major arguments).
#[inline]
#[must_use]
#[allow(clippy::too_many_arguments, clippy::similar_names)] // CGAL signature
pub fn determinant3(
    a00: Real,
    a01: Real,
    a02: Real,
    a10: Real,
    a11: Real,
    a12: Real,
    a20: Real,
    a21: Real,
    a22: Real,
) -> Real {
    let m01 = a00 * a11 - a10 * a01;
    let m02 = a00 * a21 - a20 * a01;
    let m12 = a10 * a21 - a20 * a11;
    m01 * a22 - m02 * a12 + m12 * a02
}

/// `orientationC2(p, q, r)` =
/// `sign_of_determinant(qx−px, qy−py, rx−px, ry−py)`.
#[inline]
#[must_use]
pub fn orientation2(px: Real, py: Real, qx: Real, qy: Real, rx: Real, ry: Real) -> Sign {
    sign_of_determinant2(qx - px, qy - py, rx - px, ry - py)
}

/// `Orientation_3(p, q, r, s)` (`orientationC3`).
#[inline]
#[must_use]
pub fn orientation(p: Point3, q: Point3, r: Point3, s: Point3) -> Sign {
    Sign::of(determinant3(
        q.x - p.x,
        r.x - p.x,
        s.x - p.x,
        q.y - p.y,
        r.y - p.y,
        s.y - p.y,
        q.z - p.z,
        r.z - p.z,
        s.z - p.z,
    ))
}

/// `Coplanar_orientation_3(p, q, r)` (`coplanar_orientationC3`, 3 points).
#[must_use]
pub fn coplanar_orientation3(p: Point3, q: Point3, r: Point3) -> Sign {
    let oxy = orientation2(p.x, p.y, q.x, q.y, r.x, r.y);
    if oxy != Sign::Zero {
        return oxy;
    }
    let oyz = orientation2(p.y, p.z, q.y, q.z, r.y, r.z);
    if oyz != Sign::Zero {
        return oyz;
    }
    orientation2(p.x, p.z, q.x, q.z, r.x, r.z)
}

/// `Coplanar_orientation_3(p, q, r, s)` (`coplanar_orientationC3`,
/// 4 points).
#[must_use]
pub fn coplanar_orientation4(p: Point3, q: Point3, r: Point3, s: Point3) -> Sign {
    let oxy = orientation2(p.x, p.y, q.x, q.y, r.x, r.y);
    if oxy != Sign::Zero {
        return oxy.times(orientation2(p.x, p.y, q.x, q.y, s.x, s.y));
    }
    let oyz = orientation2(p.y, p.z, q.y, q.z, r.y, r.z);
    if oyz != Sign::Zero {
        return oyz.times(orientation2(p.y, p.z, q.y, q.z, s.y, s.z));
    }
    let oxz = orientation2(p.x, p.z, q.x, q.z, r.x, r.z);
    oxz.times(orientation2(p.x, p.z, q.x, q.z, s.x, s.z))
}

/// `collinearC3(p, q, r)`.
#[must_use]
pub fn collinear(p: Point3, q: Point3, r: Point3) -> bool {
    let dpx = p.x - r.x;
    let dqx = q.x - r.x;
    let dpy = p.y - r.y;
    let dqy = q.y - r.y;
    if sign_of_determinant2(dpx, dqx, dpy, dqy) != Sign::Zero {
        return false;
    }
    let dpz = p.z - r.z;
    let dqz = q.z - r.z;
    sign_of_determinant2(dpx, dqx, dpz, dqz) == Sign::Zero
        && sign_of_determinant2(dpy, dqy, dpz, dqz) == Sign::Zero
}

/// `Collinear_are_ordered_along_line_3(p, q, r)`: `q` lies between `p`
/// and `r` (`collinear_are_ordered_along_lineC3`).
#[must_use]
#[allow(clippy::neg_cmp_op_on_partial_ord)] // CGAL's exact `!(a < b)` form (NaN semantics)
pub fn collinear_are_ordered_along_line(p: Point3, q: Point3, r: Point3) -> bool {
    if p.x < q.x {
        return !(r.x < q.x);
    }
    if q.x < p.x {
        return !(q.x < r.x);
    }
    if p.y < q.y {
        return !(r.y < q.y);
    }
    if q.y < p.y {
        return !(q.y < r.y);
    }
    if p.z < q.z {
        return !(r.z < q.z);
    }
    if q.z < p.z {
        return !(q.z < r.z);
    }
    true
}

/// `Compare_xyz_3` (`compare_lexicographically_xyzC3`).
#[inline]
#[must_use]
pub fn compare_xyz(p: Point3, q: Point3) -> Sign {
    let c = Sign::compare(p.x, q.x);
    if c != Sign::Zero {
        return c;
    }
    let c = Sign::compare(p.y, q.y);
    if c != Sign::Zero {
        return c;
    }
    Sign::compare(p.z, q.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    const O: Point3 = Point3::new(0.0, 0.0, 0.0);
    const X: Point3 = Point3::new(1.0, 0.0, 0.0);
    const Y: Point3 = Point3::new(0.0, 1.0, 0.0);
    const Z: Point3 = Point3::new(0.0, 0.0, 1.0);

    #[test]
    fn orientation_signs() {
        assert_eq!(orientation(O, X, Y, Z), Sign::Positive);
        assert_eq!(orientation(O, Y, X, Z), Sign::Negative);
        assert_eq!(orientation(O, X, Y, Point3::new(0.3, 0.3, 0.0)), Sign::Zero);
        assert_eq!(coplanar_orientation3(O, X, Y), Sign::Positive);
        assert_eq!(coplanar_orientation3(O, Y, X), Sign::Negative);
        assert_eq!(
            coplanar_orientation4(O, X, Y, Point3::new(0.5, 0.5, 0.0)),
            Sign::Positive
        );
        assert_eq!(
            coplanar_orientation4(O, X, Y, Point3::new(0.5, -0.5, 0.0)),
            Sign::Negative
        );
        assert!(collinear(O, X, Point3::new(2.0, 0.0, 0.0)));
        assert!(!collinear(O, X, Y));
    }

    #[test]
    fn ordering_along_line_and_lexicographic_compare() {
        let h = Point3::new(0.5, 0.0, 0.0);
        assert!(collinear_are_ordered_along_line(O, h, X));
        assert!(collinear_are_ordered_along_line(X, h, O));
        assert!(!collinear_are_ordered_along_line(h, O, X));
        assert_eq!(compare_xyz(O, X), Sign::Negative);
        assert_eq!(compare_xyz(Y, X), Sign::Negative);
        assert_eq!(compare_xyz(Z, Y), Sign::Negative);
        assert_eq!(compare_xyz(Z, Z), Sign::Zero);
        let s = Segment3::new(X, O);
        assert_eq!(s.min(), O);
        assert_eq!(s.max(), X);
    }

    #[test]
    fn tetrahedron_sides() {
        let t = Tetrahedron3::new(O, X, Y, Z);
        assert!(t.has_on_bounded_side(Point3::new(0.1, 0.1, 0.1)));
        assert!(!t.has_on_boundary(Point3::new(0.1, 0.1, 0.1)));
        assert!(t.has_on_boundary(Point3::new(0.1, 0.1, 0.0)));
        assert_eq!(
            t.bounded_side(Point3::new(1.0, 1.0, 1.0)),
            BoundedSide::Unbounded
        );
        // Reversed orientation gives the same sides (positive denominator).
        let r = Tetrahedron3::new(O, Y, X, Z);
        assert!(r.has_on_bounded_side(Point3::new(0.1, 0.1, 0.1)));
    }

    #[test]
    fn bbox_union_and_plane() {
        let b = Triangle3::new(O, X, Y).bbox();
        assert_eq!(b.min, [0.0, 0.0, 0.0]);
        assert_eq!(b.max, [1.0, 1.0, 0.0]);
        let p = Plane3::from_points(O, X, Y);
        assert_eq!((p.a, p.b, p.c, p.d), (0.0, 0.0, 1.0, 0.0));
    }
}
