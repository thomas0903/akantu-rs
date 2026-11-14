// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_segment_intersector_tmpl.hh
// (computeSegments: the pieces of one query) and geom_helper_functions.hh
// (the near-duplicate intent of compareSegments / IsSameSegment). The interval
// construction itself is the port's correction (UD-599, UD-601); it uses no
// CGAL code.

//! Interval tiling of the pieces of one query segment (UD-599, UD-601;
//! [`crate::PieceAssembly::Tiled`]).
//!
//! The face construction of [`crate::MeshSegmentIntersector`] (upstream's
//! `computeSegments` with the corrected tetrahedron faces of UD-407) works
//! in `Simple_cartesian<double>`, whose predicates are not exact, exactly
//! as CGAL. A query lying along a shared edge or face is reported by every
//! element sharing it, with end points that differ in the last bits; a
//! query through a mesh vertex at a grazing angle to a face produces end
//! points whose error grows like `ε/δ` (δ the angle), so near-duplicates
//! are no longer equal within any fixed point tolerance, and the vertex
//! hits on the incident faces can all be missed, losing a whole piece.
//! Comparing end points cannot repair either case.
//!
//! The tiled assembly works on the **parameter** `t ∈ [0, 1]` of each piece
//! along the query `A + t (B − A)` instead:
//!
//! 1. every candidate piece (face construction, exact duplicates already
//!    removed) gets its interval `[t₀, t₁]`; pieces not longer than the
//!    parameter tolerance `τ` are dropped (slivers); a piece is trimmed to
//!    the interval of the query inside its own element (barycentric
//!    coordinates `≥ −η`, affine along the query) where it extends beyond
//!    it by more than `τ` (an ill-conditioned end point: a vertex hit at a
//!    grazing angle can land outside the element, even outside the mesh),
//!    and dropped when it does not overlap that interval by more than `τ`;
//! 2. the pieces are sorted by `(t₀ ascending, t₁ descending, element
//!    ascending)` and swept once: a piece covered by the pieces before it
//!    (up to `τ`) is dropped, a piece overlapping them by more than `τ` is
//!    clipped (its start becomes the previous piece's end point), other
//!    pieces are kept **unchanged** (upstream's end points, so the
//!    well-conditioned cases stay bit-identical to the face construction);
//! 3. every gap longer than `τ` (including before the first and after the
//!    last piece) is located: the query is clipped against every element,
//!    the gap is split at the clip bounds (strict, `λ ≥ 0`, and within
//!    tolerance, `λ ≥ −η`), and each sub-interval goes to the element that
//!    contains its midpoint best (largest smallest barycentric coordinate)
//!    if that element contains it: strictly (computed coordinates `≥ 0`),
//!    or within `η` when the sub-interval is enclosed by material on both
//!    sides (a junction between two elements; next to the mesh boundary or
//!    a query end such a sub-interval stays a gap, so the tolerance never
//!    adds length outside the mesh); otherwise the sub-interval lies outside
//!    the mesh and stays a gap. A piece that the face construction lost is
//!    therefore rebuilt, never silently dropped.
//!
//! The pieces of a query thus cover every part of it that lies in the
//! mesh, without overlap: consecutive pieces meet within `τ` (exactly at a
//! clip or fill), and every accepted gap has its midpoint outside every
//! element (by more than the point tolerance). "In the mesh" is decided up
//! to the point tolerance: a query within `comparePoints` tolerance of the
//! mesh boundary lies on it (the face construction, with its inexact
//! predicates, may or may not report such a query). Non-finite coordinates
//! are refused with an error.
//!
//! **Tolerances.** Both decide only whether two positions denote the same
//! point, in the sense of upstream's `comparePoints` (`Math::are_float_equal`
//! on every coordinate, `|x − y| ≤ tol·max(1, |x|, |y|)`) with `tol` =
//! [`crate::SegmentIntersectorOptions::point_tolerance`] (upstream's
//! `Math::tolerance`, default `1e2·ε`):
//! * `τ = tol · max(1, ‖A‖∞, ‖B‖∞) / ‖B − A‖∞`: two parameters closer than
//!   `τ` give points that `comparePoints` cannot tell apart (common scale of
//!   the query's coordinates);
//! * `η_e = tol · max(1, ‖A‖∞, ‖B‖∞, max |vertex coordinate of e|) ·
//!   max_a ‖∇λ_a‖₁`: a point moved by at most that point tolerance changes a
//!   barycentric coordinate `λ_a` of element `e` by at most `η_e`, so a
//!   point within `comparePoints` tolerance of `e` counts as inside.

use akantu_core::{Error, Int, Real, Result};

use crate::kernel::{Point3, Segment3};

/// Counters of one tiled query ([`crate::MeshSegmentIntersector::compute_segments_with_report`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TilingReport {
    /// Candidate pieces of the face construction (after its exact
    /// deduplication).
    pub candidates: usize,
    /// Candidates kept (unchanged or clipped).
    pub kept: usize,
    /// Candidates dropped because earlier pieces cover their interval
    /// (duplicates and near-duplicates).
    pub covered: usize,
    /// Candidates dropped because their interval is not longer than `τ`
    /// (degenerate pieces and slivers).
    pub slivers: usize,
    /// Candidates dropped because they do not overlap the interval of the
    /// query inside their element by more than `τ`.
    pub invalid: usize,
    /// Kept candidates trimmed to the interval of the query inside their
    /// element (an end point beyond it by more than `τ`).
    pub trimmed: usize,
    /// Kept candidates whose start was moved to the previous piece's end.
    pub clipped: usize,
    /// Gaps (longer than `τ`) that lie at least partly in the mesh and were
    /// rebuilt by clipping.
    pub gaps_filled: usize,
    /// Pieces created by those rebuilds.
    pub fill_pieces: usize,
    /// Gaps entirely outside the mesh (accepted).
    pub outside_gaps: usize,
}

/// A background element as a simplex with the gradients of its barycentric
/// coordinates: a triangle in the `(x, y)` plane (2-D meshes, `z` ignored) or
/// a tetrahedron.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Simplex {
    v: [Point3; 4],
    nb_vertices: usize,
    /// `∇λ_a` (`z = 0` for triangles).
    grad: [[Real; 3]; 4],
    /// `max |vertex coordinate|`.
    vertex_scale: Real,
    /// `max_a ‖∇λ_a‖₁`.
    grad_norm: Real,
    /// Non-degenerate with finite gradients.
    valid: bool,
}

impl Simplex {
    /// Triangle `(a, b, c)` of a 2-D mesh (`z` ignored).
    pub(crate) fn triangle(a: Point3, b: Point3, c: Point3) -> Self {
        let (e1x, e1y) = (b.x - a.x, b.y - a.y);
        let (e2x, e2y) = (c.x - a.x, c.y - a.y);
        let det = e1x * e2y - e1y * e2x;
        let g1 = [e2y / det, -(e2x / det), 0.0];
        let g2 = [-(e1y / det), e1x / det, 0.0];
        let g0 = [-(g1[0] + g2[0]), -(g1[1] + g2[1]), 0.0];
        let v = [a, b, c, Point3::default()];
        Self::finish(v, 3, [g0, g1, g2, [0.0; 3]], det)
    }

    /// Tetrahedron `(a, b, c, d)`.
    pub(crate) fn tetrahedron(a: Point3, b: Point3, c: Point3, d: Point3) -> Self {
        let e = |p: Point3| [p.x - a.x, p.y - a.y, p.z - a.z];
        let (e1, e2, e3) = (e(b), e(c), e(d));
        let cross = |u: [Real; 3], w: [Real; 3]| {
            [
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ]
        };
        let c23 = cross(e2, e3);
        let c31 = cross(e3, e1);
        let c12 = cross(e1, e2);
        let det = e1[0] * c23[0] + e1[1] * c23[1] + e1[2] * c23[2];
        let div = |g: [Real; 3]| [g[0] / det, g[1] / det, g[2] / det];
        let (g1, g2, g3) = (div(c23), div(c31), div(c12));
        let g0 = [
            -(g1[0] + g2[0] + g3[0]),
            -(g1[1] + g2[1] + g3[1]),
            -(g1[2] + g2[2] + g3[2]),
        ];
        Self::finish([a, b, c, d], 4, [g0, g1, g2, g3], det)
    }

    fn finish(v: [Point3; 4], nb_vertices: usize, grad: [[Real; 3]; 4], det: Real) -> Self {
        let vertex_scale = v[..nb_vertices]
            .iter()
            .flat_map(|p| [p.x.abs(), p.y.abs(), p.z.abs()])
            .fold(0.0, Real::max);
        let grad_norm = grad[..nb_vertices]
            .iter()
            .map(|g| g[0].abs() + g[1].abs() + g[2].abs())
            .fold(0.0, Real::max);
        let valid = det != 0.0
            && det.is_finite()
            && grad_norm.is_finite()
            && grad[..nb_vertices]
                .iter()
                .all(|g| g.iter().all(|x| x.is_finite()));
        Self {
            v,
            nb_vertices,
            grad,
            vertex_scale,
            grad_norm,
            valid,
        }
    }

    /// Barycentric coordinates of `p` (`λ_a = ∇λ_a · (p − v_b)` with `v_b`
    /// a vertex of the face opposite `a`; entries past the vertex count are
    /// zero).
    fn barycentric(&self, p: Point3) -> [Real; 4] {
        let mut out = [0.0; 4];
        for (a, slot) in out.iter_mut().enumerate().take(self.nb_vertices) {
            let base = if a == 0 { self.v[1] } else { self.v[0] };
            let g = self.grad[a];
            *slot = g[0] * (p.x - base.x) + g[1] * (p.y - base.y) + g[2] * (p.z - base.z);
        }
        out
    }

    /// `η_e` for a query of coordinate scale `query_scale` (module docs).
    fn inside_tolerance(&self, tol: Real, query_scale: Real) -> Real {
        tol * query_scale.max(self.vertex_scale) * self.grad_norm
    }
}

/// Smallest barycentric coordinate at parameter `t` from the coordinates at
/// the query ends (affine along the query).
fn min_lambda_at(la: &[Real; 4], lb: &[Real; 4], n: usize, t: Real) -> Real {
    (0..n)
        .map(|a| la[a] + t * (lb[a] - la[a]))
        .fold(Real::INFINITY, Real::min)
}

/// Interval of `t` where all barycentric coordinates are `≥ 0` (affine
/// along the query), intersected with `[lo, hi]`.
fn clip_interval(
    la: &[Real; 4],
    lb: &[Real; 4],
    n: usize,
    lo: Real,
    hi: Real,
) -> Option<(Real, Real)> {
    let (mut t0, mut t1) = (lo, hi);
    for a in 0..n {
        let d = lb[a] - la[a];
        if d > 0.0 {
            t0 = t0.max(-la[a] / d);
        } else if d < 0.0 {
            t1 = t1.min(-la[a] / d);
        } else if la[a] < 0.0 {
            return None;
        }
    }
    (t0 < t1).then_some((t0, t1))
}

/// `λ + η` (the coordinates of the element grown by the point tolerance).
fn shift(l: &[Real; 4], eta: Real) -> [Real; 4] {
    [l[0] + eta, l[1] + eta, l[2] + eta, l[3] + eta]
}

/// One kept piece during the sweep.
#[derive(Clone, Copy, Debug)]
struct Piece {
    lo: Real,
    hi: Real,
    element: Int,
    segment: Segment3,
    /// `segment.source` is the end at `lo`.
    lo_is_source: bool,
}

impl Piece {
    fn lo_point(&self) -> Point3 {
        if self.lo_is_source {
            self.segment.source
        } else {
            self.segment.target
        }
    }

    fn hi_point(&self) -> Point3 {
        if self.lo_is_source {
            self.segment.target
        } else {
            self.segment.source
        }
    }

    fn set_lo_point(&mut self, p: Point3) {
        if self.lo_is_source {
            self.segment.source = p;
        } else {
            self.segment.target = p;
        }
    }

    fn set_hi_point(&mut self, p: Point3) {
        if self.lo_is_source {
            self.segment.target = p;
        } else {
            self.segment.source = p;
        }
    }
}

/// A gap of the sweep with the points at its ends.
#[derive(Clone, Copy, Debug)]
struct Gap {
    lo: Real,
    hi: Real,
    lo_point: Point3,
    hi_point: Point3,
    /// A kept piece ends at `lo` (not the query's start).
    lo_is_material: bool,
    /// A kept piece starts at `hi` (not the query's end).
    hi_is_material: bool,
}

fn inf_norm(p: Point3) -> Real {
    p.x.abs().max(p.y.abs()).max(p.z.abs())
}

fn finite(p: Point3) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.z.is_finite()
}

fn non_finite() -> Error {
    Error::Numerical(
        "MeshSegmentIntersector (tiled pieces, UD-601): non-finite coordinates in a query or a \
         piece"
            .into(),
    )
}

/// The query geometry shared by every step.
struct Query {
    a: Point3,
    b: Point3,
    d: [Real; 3],
    dd: Real,
    scale: Real,
    tau: Real,
}

impl Query {
    fn param(&self, p: Point3) -> Real {
        ((p.x - self.a.x) * self.d[0] + (p.y - self.a.y) * self.d[1] + (p.z - self.a.z) * self.d[2])
            / self.dd
    }

    fn point(&self, t: Real) -> Point3 {
        Point3::new(
            self.a.x + t * self.d[0],
            self.a.y + t * self.d[1],
            self.a.z + t * self.d[2],
        )
    }
}

/// Barycentric coordinates of the query ends in every element (computed
/// once per query, on the first gap).
struct EndCoordinates {
    la: Vec<[Real; 4]>,
    lb: Vec<[Real; 4]>,
}

impl EndCoordinates {
    fn new(q: &Query, simplices: &[Simplex]) -> Self {
        Self {
            la: simplices.iter().map(|s| s.barycentric(q.a)).collect(),
            lb: simplices.iter().map(|s| s.barycentric(q.b)).collect(),
        }
    }
}

/// Rebuild the part of `gap` that lies in the mesh (step 3 of the module
/// documentation); the pieces are oriented along the query.
fn fill_gap(
    q: &Query,
    gap: Gap,
    simplices: &[Simplex],
    ends: &EndCoordinates,
    tol: Real,
) -> Result<Vec<Piece>> {
    let mut breaks = vec![gap.lo, gap.hi];
    for (e, s) in simplices.iter().enumerate() {
        if !s.valid {
            continue;
        }
        let eta = s.inside_tolerance(tol, q.scale);
        let strict = clip_interval(&ends.la[e], &ends.lb[e], s.nb_vertices, gap.lo, gap.hi);
        let (la, lb) = (shift(&ends.la[e], eta), shift(&ends.lb[e], eta));
        let loose = clip_interval(&la, &lb, s.nb_vertices, gap.lo, gap.hi);
        for (t0, t1) in strict.into_iter().chain(loose) {
            breaks.push(t0);
            breaks.push(t1);
        }
    }
    if breaks.iter().any(|t| !t.is_finite()) {
        return Err(non_finite());
    }
    breaks.sort_by(Real::total_cmp);
    // merge break points closer than τ; the gap ends are kept exactly
    let mut merged: Vec<Real> = vec![gap.lo];
    for &t in &breaks {
        if t > gap.hi - q.tau {
            break;
        }
        if let Some(&last) = merged.last()
            && t > last + q.tau
        {
            merged.push(t);
        }
    }
    merged.push(gap.hi);
    let point_at = |t: Real| {
        if t == gap.lo {
            gap.lo_point
        } else if t == gap.hi {
            gap.hi_point
        } else {
            q.point(t)
        }
    };
    // classify the sub-intervals: strictly inside an element (computed
    // barycentric coordinates ≥ 0), inside only within the point tolerance,
    // or outside the mesh
    #[derive(Clone, Copy, PartialEq)]
    enum Class {
        Strict(usize),
        Band(usize),
        Outside,
    }
    let mut classes: Vec<Class> = Vec::with_capacity(merged.len().saturating_sub(1));
    for w in merged.windows(2) {
        let tm = 0.5 * (w[0] + w[1]);
        let mut best: Option<(Real, usize)> = None;
        for (e, s) in simplices.iter().enumerate() {
            if !s.valid {
                continue;
            }
            let m = min_lambda_at(&ends.la[e], &ends.lb[e], s.nb_vertices, tm);
            if best.is_none_or(|(b, _)| m > b) {
                best = Some((m, e));
            }
        }
        classes.push(match best {
            Some((m, e)) if m >= 0.0 => Class::Strict(e),
            Some((m, e)) if m >= -simplices[e].inside_tolerance(tol, q.scale) => Class::Band(e),
            _ => Class::Outside,
        });
    }
    // a run of tolerance-band sub-intervals is in the mesh only when it is
    // enclosed by material on both sides (an interior junction between two
    // elements); next to the mesh boundary or a query end it stays a gap
    let n = classes.len();
    let mut keep = vec![false; n];
    let mut k = 0;
    while k < n {
        match classes[k] {
            Class::Strict(_) => {
                keep[k] = true;
                k += 1;
            }
            Class::Outside => k += 1,
            Class::Band(_) => {
                let start = k;
                while k < n && matches!(classes[k], Class::Band(_)) {
                    k += 1;
                }
                let left = if start == 0 {
                    gap.lo_is_material
                } else {
                    matches!(classes[start - 1], Class::Strict(_))
                };
                let right = if k == n {
                    gap.hi_is_material
                } else {
                    matches!(classes[k], Class::Strict(_))
                };
                if left && right {
                    keep[start..k].iter_mut().for_each(|x| *x = true);
                }
            }
        }
    }
    let mut out: Vec<Piece> = Vec::new();
    for (j, w) in merged.windows(2).enumerate() {
        let (t0, t1) = (w[0], w[1]);
        let e = match classes[j] {
            Class::Strict(e) | Class::Band(e) if keep[j] => e,
            _ => continue,
        };
        let element = Int::try_from(e)
            .map_err(|_| Error::InvalidArgument("element index exceeds Int".into()))?;
        match out.last_mut() {
            Some(last) if last.element == element && last.hi == t0 => {
                last.hi = t1;
                last.segment.target = point_at(t1);
            }
            _ => out.push(Piece {
                lo: t0,
                hi: t1,
                element,
                segment: Segment3::new(point_at(t0), point_at(t1)),
                lo_is_source: true,
            }),
        }
    }
    Ok(out)
}

/// Tile the `candidates` of `query` (module documentation). `simplices[e]`
/// is element `e` of the intersector's element type; `tol` the point
/// tolerance. Returns the pieces in parameter order with the counters.
///
/// # Errors
/// Non-finite coordinates, or a candidate whose element index is not in
/// `simplices`.
pub(crate) fn tile(
    query: &Segment3,
    candidates: &[(Segment3, Int)],
    simplices: &[Simplex],
    tol: Real,
) -> Result<(Vec<(Segment3, Int)>, TilingReport)> {
    let mut report = TilingReport {
        candidates: candidates.len(),
        ..TilingReport::default()
    };
    let (a, b) = (query.source, query.target);
    if !finite(a) || !finite(b) {
        return Err(non_finite());
    }
    let d = [b.x - a.x, b.y - a.y, b.z - a.z];
    let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if dd == 0.0 {
        // a degenerate query has only degenerate pieces (upstream skips them)
        report.slivers = candidates.len();
        return Ok((Vec::new(), report));
    }
    let scale = 1.0_f64.max(inf_norm(a)).max(inf_norm(b));
    let d_inf = d[0].abs().max(d[1].abs()).max(d[2].abs());
    let q = Query {
        a,
        b,
        d,
        dd,
        scale,
        tau: tol * scale / d_inf,
    };

    // 1. intervals, slivers, validity
    let mut entries: Vec<Piece> = Vec::with_capacity(candidates.len());
    for &(segment, element) in candidates {
        if !finite(segment.source) || !finite(segment.target) {
            return Err(non_finite());
        }
        let (ts, tt) = (q.param(segment.source), q.param(segment.target));
        let (lo, hi, lo_is_source) = if ts <= tt {
            (ts, tt, true)
        } else {
            (tt, ts, false)
        };
        if hi - lo <= q.tau {
            report.slivers += 1;
            continue;
        }
        let simplex = usize::try_from(element)
            .ok()
            .and_then(|e| simplices.get(e))
            .ok_or_else(|| {
                Error::InvalidArgument(format!(
                    "MeshSegmentIntersector (tiled pieces): element {element} out of range"
                ))
            })?;
        // the interval of the query inside the element: within tolerance
        // (decides whether the piece extends beyond its element) and strict
        // (where it is trimmed)
        let eta = simplex.inside_tolerance(tol, q.scale);
        let (la, lb) = (simplex.barycentric(a), simplex.barycentric(b));
        let n = simplex.nb_vertices;
        let (inf, sup) = (Real::NEG_INFINITY, Real::INFINITY);
        let loose = simplex
            .valid
            .then(|| clip_interval(&shift(&la, eta), &shift(&lb, eta), n, inf, sup))
            .flatten();
        let Some((e_lo, e_hi)) = loose else {
            report.invalid += 1;
            continue;
        };
        let strict = clip_interval(&la, &lb, n, inf, sup);
        let mut piece = Piece {
            lo,
            hi,
            element,
            segment,
            lo_is_source,
        };
        let mut trimmed = false;
        if lo < e_lo - q.tau {
            piece.lo = strict.map_or(e_lo, |(t, _)| t);
            trimmed = true;
        }
        if hi > e_hi + q.tau {
            piece.hi = strict.map_or(e_hi, |(_, t)| t);
            trimmed = true;
        }
        if piece.hi - piece.lo <= q.tau {
            report.invalid += 1;
            continue;
        }
        if piece.lo != lo {
            piece.set_lo_point(q.point(piece.lo));
        }
        if piece.hi != hi {
            piece.set_hi_point(q.point(piece.hi));
        }
        report.trimmed += usize::from(trimmed);
        entries.push(piece);
    }

    // 2. sweep in parameter order
    entries.sort_by(|x, y| {
        x.lo.total_cmp(&y.lo)
            .then(y.hi.total_cmp(&x.hi))
            .then(x.element.cmp(&y.element))
    });
    let mut kept: Vec<Piece> = Vec::with_capacity(entries.len());
    let mut gaps: Vec<Gap> = Vec::new();
    let (mut end_t, mut end_p) = (0.0, a);
    for mut e in entries {
        if !kept.is_empty() && e.hi <= end_t + q.tau {
            report.covered += 1;
            continue;
        }
        if e.lo > end_t + q.tau {
            gaps.push(Gap {
                lo: end_t,
                hi: e.lo,
                lo_point: end_p,
                hi_point: e.lo_point(),
                lo_is_material: !kept.is_empty(),
                hi_is_material: true,
            });
        } else if !kept.is_empty() && e.lo < end_t - q.tau {
            // overlap beyond τ: keep only the uncovered part
            e.set_lo_point(end_p);
            e.lo = end_t;
            report.clipped += 1;
        }
        end_t = e.hi;
        end_p = e.hi_point();
        kept.push(e);
    }
    if end_t < 1.0 - q.tau {
        gaps.push(Gap {
            lo: end_t,
            hi: 1.0,
            lo_point: end_p,
            hi_point: b,
            lo_is_material: !kept.is_empty(),
            hi_is_material: false,
        });
    }
    report.kept = kept.len();

    // 3. gaps: rebuild what lies in the mesh
    let mut fills: Vec<Piece> = Vec::new();
    if !gaps.is_empty() {
        let ends = EndCoordinates::new(&q, simplices);
        for gap in gaps {
            let pieces = fill_gap(&q, gap, simplices, &ends, tol)?;
            if pieces.is_empty() {
                report.outside_gaps += 1;
            } else {
                report.gaps_filled += 1;
                report.fill_pieces += pieces.len();
                fills.extend(pieces);
            }
        }
    }
    kept.extend(fills);
    kept.sort_by(|x, y| x.lo.total_cmp(&y.lo));
    Ok((
        kept.into_iter().map(|p| (p.segment, p.element)).collect(),
        report,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: Real, y: Real, z: Real) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn barycentric_coordinates_of_simplices() {
        let t = Simplex::triangle(p(0., 0., 0.), p(2., 0., 0.), p(0., 1., 0.));
        let l = t.barycentric(p(0.5, 0.25, 0.));
        assert_eq!(&l[..3], &[0.5, 0.25, 0.25]);
        let tet = Simplex::tetrahedron(p(0., 0., 0.), p(1., 0., 0.), p(0., 1., 0.), p(0., 0., 1.));
        let l = tet.barycentric(p(0.25, 0.25, 0.25));
        assert_eq!(l, [0.25, 0.25, 0.25, 0.25]);
        assert!(tet.valid && t.valid);
        let flat = Simplex::tetrahedron(p(0., 0., 0.), p(1., 0., 0.), p(0., 1., 0.), p(1., 1., 0.));
        assert!(!flat.valid);
    }

    #[test]
    fn clip_interval_of_a_triangle() {
        let t = Simplex::triangle(p(0., 0., 0.), p(1., 0., 0.), p(0., 1., 0.));
        let (a, b) = (p(-0.5, 0.25, 0.), p(1.5, 0.25, 0.));
        let (la, lb) = (t.barycentric(a), t.barycentric(b));
        let (t0, t1) = clip_interval(&la, &lb, 3, 0., 1.).unwrap();
        assert_eq!((t0, t1), (0.25, 0.625));
        assert!(clip_interval(&la, &lb, 3, 0.7, 1.).is_none());
    }

    fn unit_triangles() -> Vec<Simplex> {
        // two triangles of the unit square, diagonal (0,0)–(1,1)
        vec![
            Simplex::triangle(p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.)),
            Simplex::triangle(p(0., 0., 0.), p(1., 1., 0.), p(0., 1., 0.)),
        ]
    }

    #[test]
    fn duplicates_overlaps_slivers_and_gaps() {
        let s = unit_triangles();
        let q = Segment3::new(p(-0.5, 0.5, 0.), p(1.5, 0.5, 0.));
        let seg = |x0: Real, x1: Real| Segment3::new(p(x0, 0.5, 0.), p(x1, 0.5, 0.));
        // the query runs from x = −0.5 to 1.5: in triangle 1 for x ∈ [0, 0.5],
        // triangle 0 for x ∈ [0.5, 1]
        let candidates = [
            (seg(0.5, 1.0), 0),
            (seg(1.0, 0.5), 0),         // exact duplicate, reversed
            (seg(0.0, 0.5 + 1e-16), 1), // near-duplicate end
            (seg(0.25, 0.5), 1),        // covered
            (seg(0.5, 0.5 + 1e-17), 0), // degenerate after rounding
        ];
        let (pieces, r) = tile(&q, &candidates, &s, 1e2 * Real::EPSILON).unwrap();
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        assert_eq!(r.covered, 2);
        assert_eq!(r.slivers, 1);
        assert_eq!(r.outside_gaps, 2);
        assert_eq!(r.gaps_filled, 0);
        // a lost piece is rebuilt from the clip of the element containing it
        let (pieces, r) = tile(&q, &[(seg(0.5, 1.0), 0)], &s, 1e2 * Real::EPSILON).unwrap();
        assert_eq!(r.gaps_filled, 1);
        assert_eq!(pieces.len(), 2);
        assert_eq!(
            pieces[0],
            (Segment3::new(p(0., 0.5, 0.), p(0.5, 0.5, 0.)), 1)
        );
        // an end point beyond the element is trimmed to the element's interval
        let (pieces, r) = tile(
            &q,
            &[(seg(0.0, 0.6), 1), (seg(0.5, 1.0), 0)],
            &s,
            1e2 * Real::EPSILON,
        )
        .unwrap();
        assert_eq!((r.trimmed, r.clipped), (1, 0));
        assert_eq!(pieces[0].0, Segment3::new(p(0.0, 0.5, 0.), p(0.5, 0.5, 0.)));
        // overlap beyond τ (two coincident elements): the later piece is
        // clipped at the earlier end
        let twice = vec![s[1], s[1]];
        let (pieces, r) = tile(
            &q,
            &[(seg(0.0, 0.4), 0), (seg(0.2, 0.5), 1)],
            &twice,
            1e2 * Real::EPSILON,
        )
        .unwrap();
        assert_eq!(r.clipped, 1);
        assert_eq!(
            pieces[1],
            (Segment3::new(p(0.4, 0.5, 0.), p(0.5, 0.5, 0.)), 1)
        );
        // non-finite input
        let bad = Segment3::new(p(Real::NAN, 0., 0.), p(1., 0., 0.));
        assert!(tile(&bad, &[], &s, 1e-14).is_err());
    }
}
