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

/// Largest number of cell items per primitive and cell (on average) a grid
/// may hold; beyond, [`GridIndex`] scans (memory bound for meshes whose
/// primitives span many cells, e.g. a few huge elements among many small).
const ITEMS_PER_PRIMITIVE: u64 = 64;

/// Cube root of a positive finite `v` with a fixed sequence of IEEE
/// operations (the same result on every platform; not necessarily
/// correctly rounded, which the cell size does not need): a power-of-two
/// start from the exponent, then a fixed number of Newton steps.
fn cube_root(v: Real) -> Real {
    if !(v.is_finite() && v > 0.0) {
        return v;
    }
    // 2^(e/3) with e the binary exponent of v: within a factor 2 of the root
    // (built from its bits: exact, no library call)
    let e = i64::try_from((v.to_bits() >> 52) & 0x7ff).unwrap_or(1023) - 1023;
    let bits = u64::try_from(e / 3 + 1023).unwrap_or(1023) << 52;
    let mut x = Real::from_bits(bits);
    for _ in 0..12 {
        x -= (x * x * x - v) / (3.0 * x * x);
    }
    x
}

/// Uniform-grid search structure with the reports of [`LinearScan`] (LGPL).
///
/// **Structure.** The cells cover the union `B` of the primitives' bounding
/// boxes, about one primitive per cell and not smaller than half the mean
/// primitive extent (directions in which `B` is flat, such as `z` of a 2-D
/// mesh, get one cell). Every primitive is listed, in primitive order, in
/// each cell its bounding box overlaps. A cell coordinate is
/// `clamp(floor((x − B_min) / h), 0, n − 1)`, a monotone function of `x`,
/// so a query box grown by `δ` reaches every cell of every primitive whose
/// box it overlaps.
///
/// **Queries.** A segment or ray is clipped to the parameter range of the
/// projections of the corners of `B` (every primitive point projects inside
/// it), then cut into pieces about one cell long; each piece's bounding box,
/// grown by `δ_q = δ + 8 ε max|piece coordinate| + e` (the rounding of the
/// cut points, and `e` that of the clipped end points), selects cells. The
/// candidates are sorted by primitive order, duplicates removed, and tested
/// with the scan's predicates ([`triangle_segment_intersection`],
/// [`ray_triangle_do_intersect`]) in that order, so the output is the scan's
/// output restricted to the candidates, in the scan's order.
///
/// **What the restriction removes.** Every primitive whose bounding box
/// comes within `δ = h / 4` (a quarter of the cell edge
/// `h = max((V / n_p)^(1/d), ē / 2)`, `V` the volume of `B` in its `d`
/// non-flat directions, `n_p` the primitive count, `ē` the mean primitive
/// extent) of the query is a candidate. So the grid reports the scan's
/// reports, in the scan's order, minus those of primitives farther away.
/// The scan reports such a primitive only through rounding noise in a
/// configuration that is degenerate up to rounding (the query on the
/// supporting plane of a distant face or on the supporting line of a
/// distant edge); the report can be a point or piece outside the query, or
/// the query itself attached to the distant primitive. A caller that
/// deduplicates pieces by element index (the segment intersector's
/// collected set) can therefore end up with another element for a piece:
/// the one containing it, where the scan's lower-index rule keeps the
/// distant one. Upstream's AABB tree (its box test) does not report the
/// distant primitive either, and keeps the grid's element (the
/// CGAL-compatible tree of `akantu-geometry-cgal` gives the same pieces on
/// such queries). The same holds for the scan's `CGAL_error()` on a
/// degenerate primitive (collinear vertices) lying on the query's line: the
/// grid returns the error only when such a primitive is a candidate. Non-finite coordinates in the
/// primitives (at build) or in a query, and a zero-length segment (its
/// result, an error or nothing, depends on every primitive coplanar with
/// the point), make that query a full scan; so does
/// a grid whose cell items would exceed `u32` or 64 per primitive and cell
/// on average (the index then scans every query).
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
        for p in primitives {
            for v in &p.triangle.v {
                if !finite3(v) {
                    return None;
                }
                for (d, x) in coords(v).into_iter().enumerate() {
                    lo[d] = lo[d].min(x);
                    hi[d] = hi[d].max(x);
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
        // the cell edge of one primitive per cell: a root of the volume per
        // primitive, with correctly rounded or fixed-sequence IEEE operations
        // only (no platform libm), so the grid is the same on every platform
        let v = vol / target;
        let h_volume = match active.len() {
            1 => v,
            2 => v.sqrt(),
            _ => cube_root(v),
        };
        // cells not much smaller than the primitives (a primitive spans
        // about two cells per direction at most)
        let mean_extent = primitives
            .iter()
            .map(|p| {
                let b = p.triangle.bbox();
                (0..3).fold(0.0, |m: Real, d| m.max(b.max[d] - b.min[d]))
            })
            .sum::<Real>()
            / target;
        let h = h_volume.max(0.5 * mean_extent);
        if !(h.is_finite() && h > 0.0) {
            return None;
        }
        let mut n = [1usize; 3];
        let mut total = 1usize;
        for &d in &active {
            let c = (ext[d] / h).ceil().clamp(1.0, 4096.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // clamped to [1, 4096]
            let c = c as usize;
            n[d] = c;
            total *= c;
        }
        let delta = 0.25 * h;
        // the item count of every primitive's cell range, checked: beyond
        // `u32` (the counters) or beyond `ITEMS_PER_PRIMITIVE` items per
        // primitive and cell on average (memory), the index scans instead
        let probe = Self {
            lo,
            hi,
            h,
            n,
            delta,
            start: Vec::new(),
            items: Vec::new(),
        };
        let ranges: Vec<[[usize; 2]; 3]> = primitives
            .iter()
            .map(|p| {
                let b = p.triangle.bbox();
                probe.cell_range(&b.min, &b.max, 0.0)
            })
            .collect();
        let nb_items = ranges.iter().try_fold(0u64, |acc, r| {
            let k = r.iter().try_fold(1u64, |m, [a, b]| {
                m.checked_mul(u64::try_from(b - a + 1).ok()?)
            })?;
            acc.checked_add(k)
        })?;
        let cap = u64::try_from(np.saturating_add(total))
            .ok()?
            .saturating_mul(ITEMS_PER_PRIMITIVE);
        if nb_items >= u64::from(u32::MAX) || nb_items > cap {
            return None;
        }
        let mut g = Self {
            lo,
            hi,
            h,
            n,
            delta,
            start: vec![0; total + 1],
            items: Vec::new(),
        };
        // two passes: counts, then fill (items of a cell in primitive order);
        // a primitive is listed in the cells its exact box overlaps (the
        // margin is added on the query side); the counts cannot overflow
        // (`nb_items < u32::MAX`, checked above)
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
        self.walk(a, b, Some(b), out);
    }

    /// Candidates of the ray from `s` through `q` (sorted, unique);
    /// `false` when the ray needs a full scan (degenerate or non-finite).
    fn ray_candidates(&self, s: [Real; 3], q: [Real; 3], out: &mut Vec<u32>) -> bool {
        let d = [q[0] - s[0], q[1] - s[1], q[2] - s[2]];
        let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        if !(dd.is_finite() && dd > 0.0) {
            return false;
        }
        self.walk(s, q, None, out);
        true
    }

    /// Candidates of the points `s + t (q − s)` for `t ∈ [0, 1]` (segment,
    /// `end = Some(q)`) or `t ≥ 0` (ray, `end = None`).
    ///
    /// The parameter range is first clipped to the projections of the
    /// corners of `B` (every primitive point lies in `B`, so its parameter
    /// lies in that range); a range that misses them, up to rounding, is
    /// reduced to the nearest end of the query. The clipped part is cut
    /// into pieces about one cell long; each piece's box is grown by
    /// `δ_q = δ + 8 ε max|piece coordinate| + e`, where the second term
    /// bounds the rounding of the cut points (below) and `e` that of the
    /// clipped end points and of the corner projections.
    fn walk(&self, s: [Real; 3], q: [Real; 3], end: Option<[Real; 3]>, out: &mut Vec<u32>) {
        let d = [q[0] - s[0], q[1] - s[1], q[2] - s[2]];
        let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let tmax = if end.is_some() { 1.0 } else { Real::INFINITY };
        // parameter range of the projections of the corners of `B`
        let (mut c0, mut c1) = (Real::INFINITY, Real::NEG_INFINITY);
        let mut cmax: Real = 0.0;
        if dd > 0.0 && dd.is_finite() {
            for corner in 0..8 {
                let c = [
                    if corner & 1 == 0 {
                        self.lo[0]
                    } else {
                        self.hi[0]
                    },
                    if corner & 2 == 0 {
                        self.lo[1]
                    } else {
                        self.hi[1]
                    },
                    if corner & 4 == 0 {
                        self.lo[2]
                    } else {
                        self.hi[2]
                    },
                ];
                let e = [c[0] - s[0], c[1] - s[1], c[2] - s[2]];
                cmax = cmax.max(e[0].abs()).max(e[1].abs()).max(e[2].abs());
                let t = (e[0] * d[0] + e[1] * d[1] + e[2] * d[2]) / dd;
                c0 = c0.min(t);
                c1 = c1.max(t);
            }
        } else {
            // a point (zero-length segment): no clipping
            c0 = 0.0;
            c1 = tmax;
        }
        let (t0, t1) = if c1 < 0.0 {
            (0.0, 0.0)
        } else if c0 > tmax {
            (tmax, tmax)
        } else {
            (c0.max(0.0), c1.min(tmax))
        };
        let at = |t: Real| -> [Real; 3] {
            if t == 0.0 {
                s
            } else if let (Some(b), true) = (end, t == 1.0) {
                b
            } else {
                [s[0] + t * d[0], s[1] + t * d[1], s[2] + t * d[2]]
            }
        };
        // rounding of a clipped end point `s + t d` (`d` itself rounded) and
        // of the corner projections (a parameter error of a few
        // `ε |c − s| / |d|`, i.e. a few `ε |c − s|` along the query): a few ε
        // of `|s|`, `|t d|` and `|c − s|`, bounded with a factor 2
        let clipped = t0 != 0.0 || (t1 != 1.0 || end.is_none());
        let extra = if clipped {
            let smax = s[0].abs().max(s[1].abs()).max(s[2].abs());
            let dmax = d[0].abs().max(d[1].abs()).max(d[2].abs());
            8.0 * Real::EPSILON * (smax + t1 * dmax + cmax)
        } else {
            0.0
        };
        let (a, b) = (at(t0), at(t1));
        let qscale = a.iter().chain(&b).fold(0.0, |m: Real, x| m.max(x.abs()));
        // the cut points `a + t (b − a)` carry at most `|a| ε/2` (sum),
        // `|t (b − a)| ε/2` (product), `|b − a| ε/2` (difference, scaled by
        // `t ≤ 1`) and the rounding of `t` (`ε/2` relative): below
        // `(|a| + 3 |b − a|) ε ≤ 7 ε max|coordinate|`
        let delta = self.delta + 8.0 * Real::EPSILON * qscale + extra;
        let span = (0..3).fold(0.0, |m: Real, k| m.max((b[k] - a[k]).abs()));
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
                out.extend_from_slice(
                    &self.items[self.start[c] as usize..self.start[c + 1] as usize],
                );
            });
            p0 = p1;
        }
        out.sort_unstable();
        out.dedup();
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

    /// Lower corner, cell size and cells per direction of the grid (`None`
    /// without a grid).
    #[must_use]
    pub fn cell_layout(&self) -> Option<([Real; 3], Real, [usize; 3])> {
        self.grid.as_ref().map(|g| (g.lo, g.h, g.n))
    }

    /// Cell count and cell-item count of the grid (`None` without a grid).
    #[must_use]
    pub fn grid_size(&self) -> Option<(usize, usize)> {
        self.grid
            .as_ref()
            .map(|g| (g.start.len().saturating_sub(1), g.items.len()))
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
            // a zero-length query is a full scan: the scan's result for it
            // depends on whether any primitive is coplanar with the point
            // (the `CGAL_error()` branch of the coplanar case), near or not
            Some(g)
                if finite3(&query.source)
                    && finite3(&query.target)
                    && query.source != query.target =>
            {
                g
            }
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

#[cfg(test)]
mod tests {
    use super::cube_root;

    /// The cube root of the cell size: exact cubes come back exactly, other
    /// values within an ulp or two of the platform's `cbrt`.
    #[test]
    fn cube_root_of_cell_volumes() {
        for k in [1.0f64, 2.0, 3.0, 0.5, 0.25, 0.125, 1e-3, 7.0] {
            assert_eq!(cube_root(k * k * k), k, "{k}");
        }
        let mut v = 1e-12;
        while v < 1e12 {
            let r = cube_root(v);
            assert!((r - v.cbrt()).abs() <= 4.0 * f64::EPSILON * r, "{v}: {r}");
            v *= 1.37;
        }
    }
}
