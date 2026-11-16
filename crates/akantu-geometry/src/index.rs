// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/tree_type_helper.hh
// (TreeTypeHelper / IntersectionTypeHelper for Triangle<cgal::Cartesian>
// primitives and Segment_3 queries) and the AABB-tree calls of
// mesh_segment_intersector_tmpl.hh (all_intersections,
// all_intersected_primitives).

//! Search structures over triangle primitives (GEOM-003).
//!
//! [`TriangleIndex`] is the interface upstream's code uses on its CGAL AABB
//! tree. Three implementations exist:
//!
//! * [`LinearScan`] (this crate, LGPL): tests every primitive in primitive
//!   order. It reports every intersection the tree reports, in a different
//!   order (unobservable with the default, corrected segment ordering of
//!   the intersector). The tree can additionally miss a primitive whose
//!   bounding box CGAL's floating-point box test rejects although the
//!   triangle test would hit it (rounding-borderline contacts only).
//! * [`GridIndex`] (this crate, LGPL): a uniform grid over the primitive
//!   bounding boxes that hands each query only the primitives near it, and
//!   tests them **in primitive order with the same predicates** as
//!   [`LinearScan`]; see its documentation for what "near" means and why
//!   the reports are those of the scan.
//! * `akantu_geometry_cgal::AabbTree` (GPL-3.0-or-later, opt-in, D-042): a
//!   port of CGAL's `AABB_tree` that reproduces upstream's traversal order
//!   (needed for bit-identical output in upstream-compatibility mode).

use akantu_core::{Real, Result, UInt};

use crate::factory::TrianglePrimitive;
use crate::intersections::{
    TriangleSegmentIntersection, ray_triangle_do_intersect, triangle_segment_intersection,
};
use crate::kernel::{Ray3, Segment3};

/// Queries upstream issues on its `AABB_tree<Triangle_primitive>`.
pub trait TriangleIndex: Sized {
    /// Build over `primitives` (copied, in this order).
    ///
    /// # Errors
    /// Implementation specific.
    fn build(primitives: &[TrianglePrimitive]) -> Result<Self>;

    /// `all_intersections(segment)`: every primitive intersected, with the
    /// intersection object, in the structure's reporting order.
    ///
    /// # Errors
    /// A `CGAL_error()` branch of the intersection construction.
    fn all_intersections(
        &self,
        query: &Segment3,
        out: &mut Vec<(TriangleSegmentIntersection, UInt)>,
    ) -> Result<()>;

    /// `all_intersected_primitives(ray)`: ids of the primitives hit by the
    /// ray, in reporting order.
    fn all_intersected_primitives(&self, query: &Ray3, out: &mut Vec<UInt>);
}

/// Exhaustive search in primitive order (LGPL; see the module docs).
#[derive(Clone, Debug, Default)]
pub struct LinearScan {
    primitives: Vec<TrianglePrimitive>,
}

impl TriangleIndex for LinearScan {
    fn build(primitives: &[TrianglePrimitive]) -> Result<Self> {
        Ok(Self {
            primitives: primitives.to_vec(),
        })
    }

    fn all_intersections(
        &self,
        query: &Segment3,
        out: &mut Vec<(TriangleSegmentIntersection, UInt)>,
    ) -> Result<()> {
        for p in &self.primitives {
            if let Some(i) = triangle_segment_intersection(&p.triangle, query)? {
                out.push((i, p.id));
            }
        }
        Ok(())
    }

    fn all_intersected_primitives(&self, query: &Ray3, out: &mut Vec<UInt>) {
        for p in &self.primitives {
            if ray_triangle_do_intersect(&p.triangle, query) {
                out.push(p.id);
            }
        }
    }
}

/// Below this many primitives [`GridIndex`] scans (no grid).
const GRID_MIN_PRIMITIVES: usize = 32;

/// Uniform-grid search structure with the reports of [`LinearScan`] (LGPL).
///
/// **Structure.** The cells cover the union `B` of the primitives' bounding
/// boxes, about one primitive per cell (directions in which `B` is flat,
/// such as `z` of a 2-D mesh, get one cell). Every primitive is listed, in
/// primitive order, in each cell its bounding box grown by the margin `δ`
/// overlaps. A cell coordinate is `clamp(floor((x − B_min) / h), 0, n − 1)`,
/// a monotone function of `x`, so a box grown by `δ` reaches every cell of
/// every primitive whose grown box it overlaps.
///
/// **Queries.** A segment is cut into pieces about one cell long; each
/// piece's bounding box, grown by `δ_q = δ + 64 ε max|query coordinate|`
/// (the rounding of the cut points), selects cells. A ray is first clipped
/// to the parameter range in which its points project onto `B` (grown),
/// then walked as a segment. The candidates are sorted by primitive order,
/// duplicates removed, and tested with the scan's predicates
/// ([`triangle_segment_intersection`], [`ray_triangle_do_intersect`]) in
/// that order, so the output is the scan's output restricted to the
/// candidates, in the scan's order.
///
/// **Why the restriction removes nothing.** Every primitive that comes
/// within `δ = h / 4` (a quarter of a cell, in each coordinate) of the
/// query is a candidate. A primitive farther away is reported by the scan
/// only if the predicates' floating-point orientations are wrong by a
/// distance of a quarter cell, which takes a configuration that is
/// degenerate up to rounding (the query on the supporting plane of a
/// distant face or on the supporting line of a distant edge); such a report
/// is a point or piece outside the query, which upstream's AABB tree (its
/// box test) does not produce either. Non-finite coordinates in the
/// primitives (at build) or in a query make that query a full scan.
#[derive(Clone, Debug, Default)]
pub struct GridIndex {
    primitives: Vec<TrianglePrimitive>,
    grid: Option<Grid>,
}

#[derive(Clone, Debug)]
struct Grid {
    /// Lower corner of the cell array (`B_min`).
    lo: [Real; 3],
    /// Upper corner of `B`.
    hi: [Real; 3],
    /// Cell size (all directions with more than one cell).
    h: Real,
    /// Cells per direction.
    n: [usize; 3],
    /// Margin `δ`.
    delta: Real,
    /// `max |coordinate|` of the primitives.
    scale: Real,
    /// CSR: items of cell `c` are `items[start[c]..start[c + 1]]`.
    start: Vec<u32>,
    items: Vec<u32>,
}

fn finite3(p: &crate::kernel::Point3) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.z.is_finite()
}

fn coords(p: &crate::kernel::Point3) -> [Real; 3] {
    [p.x, p.y, p.z]
}

impl Grid {
    fn build(primitives: &[TrianglePrimitive]) -> Option<Self> {
        let np = primitives.len();
        if np < GRID_MIN_PRIMITIVES || u32::try_from(np).is_err() {
            return None;
        }
        let mut lo = [Real::INFINITY; 3];
        let mut hi = [Real::NEG_INFINITY; 3];
        let mut scale: Real = 0.0;
        for p in primitives {
            for v in &p.triangle.v {
                if !finite3(v) {
                    return None;
                }
                for (d, x) in coords(v).into_iter().enumerate() {
                    lo[d] = lo[d].min(x);
                    hi[d] = hi[d].max(x);
                    scale = scale.max(x.abs());
                }
            }
        }
        let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let l = ext[0].max(ext[1]).max(ext[2]);
        if !(l.is_finite() && l > 0.0) {
            return None;
        }
        // directions that are not flat (relative to the largest extent)
        let active: Vec<usize> = (0..3).filter(|&d| ext[d] > 1e-9 * l).collect();
        let vol: Real = active.iter().map(|&d| ext[d]).product();
        #[allow(clippy::cast_precision_loss)] // a cell-count target, any rounding is fine
        let target = np as Real;
        #[allow(clippy::cast_precision_loss)] // the dimension count (1 to 3)
        let h = (vol / target).powf(1.0 / active.len() as Real);
        if !(h.is_finite() && h > 0.0) {
            return None;
        }
        let mut n = [1usize; 3];
        let mut total = 1usize;
        for &d in &active {
            let c = (ext[d] / h).ceil().clamp(1.0, 4096.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to [1, 4096]
            let c = c as usize;
            n[d] = c;
            total *= c;
        }
        // the margin: the largest extent of a primitive's bounding box (at
        // least a quarter cell)
        let mut delta = 0.25 * h;
        for p in primitives {
            let b = p.triangle.bbox();
            for d in 0..3 {
                delta = delta.max(b.max[d] - b.min[d]);
            }
        }
        let mut g = Self {
            lo,
            hi,
            h,
            n,
            delta,
            scale,
            start: vec![0; total + 1],
            items: Vec::new(),
        };
        // two passes: counts, then fill (items of a cell in primitive order)
        let ranges: Vec<[[usize; 2]; 3]> = primitives
            .iter()
            .map(|p| {
                let b = p.triangle.bbox();
                g.cell_range(&b.min, &b.max, delta)
            })
            .collect();
        let n = g.n;
        for r in &ranges {
            let start = &mut g.start;
            Self::for_cells_static(&n, r, |c| start[c + 1] += 1);
        }
        for c in 0..total {
            g.start[c + 1] += g.start[c];
        }
        let mut fill: Vec<u32> = g.start[..total].to_vec();
        g.items = vec![0; g.start[total] as usize];
        for (i, r) in ranges.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)] // np fits u32 (checked)
            let id = i as u32;
            let (items, fill) = (&mut g.items, &mut fill);
            Self::for_cells_static(&n, r, |c| {
                items[fill[c] as usize] = id;
                fill[c] += 1;
            });
        }
        Some(g)
    }

    /// Cell coordinate of `x` in direction `d` (monotone in `x`).
    fn cell(&self, x: Real, d: usize) -> usize {
        if self.n[d] == 1 {
            return 0;
        }
        let t = ((x - self.lo[d]) / self.h).floor();
        #[allow(clippy::cast_precision_loss)] // n ≤ 4096
        let max = (self.n[d] - 1) as Real;
        let t = t.clamp(0.0, max);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to [0, n − 1]
        let t = t as usize;
        t
    }

    fn cell_range(&self, min: &[Real; 3], max: &[Real; 3], delta: Real) -> [[usize; 2]; 3] {
        let mut r = [[0; 2]; 3];
        for d in 0..3 {
            r[d] = [self.cell(min[d] - delta, d), self.cell(max[d] + delta, d)];
        }
        r
    }

    fn for_cells_static(n: &[usize; 3], r: &[[usize; 2]; 3], mut f: impl FnMut(usize)) {
        for k in r[2][0]..=r[2][1] {
            for j in r[1][0]..=r[1][1] {
                let base = (k * n[1] + j) * n[0];
                for i in r[0][0]..=r[0][1] {
                    f(base + i);
                }
            }
        }
    }

    fn for_cells(&self, r: &[[usize; 2]; 3], f: impl FnMut(usize)) {
        Self::for_cells_static(&self.n, r, f);
    }

    /// Candidates of the segment `a`–`b` (sorted, unique).
    fn segment_candidates(&self, a: [Real; 3], b: [Real; 3], out: &mut Vec<u32>) {
        let qscale = a.iter().chain(&b).fold(0.0, |m: Real, x| m.max(x.abs()));
        let delta = self.delta + 64.0 * Real::EPSILON * qscale.max(self.scale);
        let span = (0..3).fold(0.0, |m: Real, d| m.max((b[d] - a[d]).abs()));
        let steps = (span / self.h).ceil().clamp(1.0, 16384.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
        let k = steps as usize;
        let point = |i: usize| -> [Real; 3] {
            if i == 0 {
                a
            } else if i == k {
                b
            } else {
                #[allow(clippy::cast_precision_loss)] // i ≤ k ≤ 16384
                let t = i as Real / k as Real;
                [
                    a[0] + t * (b[0] - a[0]),
                    a[1] + t * (b[1] - a[1]),
                    a[2] + t * (b[2] - a[2]),
                ]
            }
        };
        let mut p0 = point(0);
        for i in 1..=k {
            let p1 = point(i);
            let min = [p0[0].min(p1[0]), p0[1].min(p1[1]), p0[2].min(p1[2])];
            let max = [p0[0].max(p1[0]), p0[1].max(p1[1]), p0[2].max(p1[2])];
            let r = self.cell_range(&min, &max, delta);
            self.for_cells(&r, |c| {
                out.extend_from_slice(&self.items[self.start[c] as usize..self.start[c + 1] as usize]);
            });
            p0 = p1;
        }
        out.sort_unstable();
        out.dedup();
    }

    /// Candidates of the ray from `s` through `q` (sorted, unique), or
    /// `None` when the ray needs a full scan.
    fn ray_candidates(&self, s: [Real; 3], q: [Real; 3], out: &mut Vec<u32>) -> bool {
        let d = [q[0] - s[0], q[1] - s[1], q[2] - s[2]];
        let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        if !(dd.is_finite() && dd > 0.0) {
            return false;
        }
        // parameter range of the projections of the (grown) box corners
        let g = self.delta;
        let (mut t0, mut t1) = (Real::INFINITY, Real::NEG_INFINITY);
        for corner in 0..8 {
            let c = [
                if corner & 1 == 0 { self.lo[0] - g } else { self.hi[0] + g },
                if corner & 2 == 0 { self.lo[1] - g } else { self.hi[1] + g },
                if corner & 4 == 0 { self.lo[2] - g } else { self.hi[2] + g },
            ];
            let t = ((c[0] - s[0]) * d[0] + (c[1] - s[1]) * d[1] + (c[2] - s[2]) * d[2]) / dd;
            t0 = t0.min(t);
            t1 = t1.max(t);
        }
        let pad = 1e-6 * (t1 - t0).abs() + 1e-6;
        let (t0, t1) = ((t0 - pad).max(0.0), t1 + pad);
        if !(t0.is_finite() && t1.is_finite()) {
            return false;
        }
        if t1 < 0.0 {
            // the whole box lies behind the source
            return true;
        }
        let at = |t: Real| [s[0] + t * d[0], s[1] + t * d[1], s[2] + t * d[2]];
        self.segment_candidates(at(t0), at(t1), out);
        true
    }
}

impl GridIndex {
    /// Primitive count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.primitives.len()
    }

    /// No primitive.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
    }

    /// The margin `δ` (`None` without a grid).
    #[must_use]
    pub fn margin(&self) -> Option<Real> {
        self.grid.as_ref().map(|g| g.delta)
    }

    /// `true` when queries use the grid (enough primitives, all finite).
    #[must_use]
    pub fn has_grid(&self) -> bool {
        self.grid.is_some()
    }
}

impl TriangleIndex for GridIndex {
    fn build(primitives: &[TrianglePrimitive]) -> Result<Self> {
        Ok(Self {
            primitives: primitives.to_vec(),
            grid: Grid::build(primitives),
        })
    }

    fn all_intersections(
        &self,
        query: &Segment3,
        out: &mut Vec<(TriangleSegmentIntersection, UInt)>,
    ) -> Result<()> {
        let grid = match &self.grid {
            Some(g) if finite3(&query.source) && finite3(&query.target) => g,
            _ => {
                for p in &self.primitives {
                    if let Some(i) = triangle_segment_intersection(&p.triangle, query)? {
                        out.push((i, p.id));
                    }
                }
                return Ok(());
            }
        };
        let mut cand = Vec::new();
        grid.segment_candidates(coords(&query.source), coords(&query.target), &mut cand);
        for &i in &cand {
            let p = &self.primitives[i as usize];
            if let Some(x) = triangle_segment_intersection(&p.triangle, query)? {
                out.push((x, p.id));
            }
        }
        Ok(())
    }

    fn all_intersected_primitives(&self, query: &Ray3, out: &mut Vec<UInt>) {
        let mut cand = Vec::new();
        let gridded = match &self.grid {
            Some(g) if finite3(&query.source) && finite3(&query.second) => {
                g.ray_candidates(coords(&query.source), coords(&query.second), &mut cand)
            }
            _ => false,
        };
        if gridded {
            for &i in &cand {
                let p = &self.primitives[i as usize];
                if ray_triangle_do_intersect(&p.triangle, query) {
                    out.push(p.id);
                }
            }
        } else {
            for p in &self.primitives {
                if ray_triangle_do_intersect(&p.triangle, query) {
                    out.push(p.id);
                }
            }
        }
    }
}
